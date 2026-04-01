use anyhow::{Context, Result};
use chrono::Utc;
use minimemory::{Config, Distance, Filter, Metadata, MetadataValue, VectorDB};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::types::*;

const EMBEDDING_DIM: usize = 64;
const MAX_MEMORIES: usize = 500;

/// minimemory-backed typed memory store.
/// Replaces KAIROS's filesystem-based approach with a vector DB that supports:
/// - Typed memories (user/feedback/project/reference/session)
/// - BM25 keyword search
/// - Vector similarity search
/// - Metadata filters ($eq, $gt, $regex)
/// - Automatic persistence (.mmdb with CRC32)
pub struct MemoryStore {
    db: VectorDB,
    data_dir: PathBuf,
}

fn make_config() -> Config {
    Config::new(EMBEDDING_DIM).with_distance(Distance::Cosine)
}

fn meta_str(meta: &Metadata, key: &str) -> String {
    match meta.get(key) {
        Some(MetadataValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn meta_int(meta: &Metadata, key: &str) -> i64 {
    match meta.get(key) {
        Some(MetadataValue::Int(n)) => *n,
        _ => 0,
    }
}

impl MemoryStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("memory.mmdb");

        let db = if db_path.exists() {
            VectorDB::open(&db_path).unwrap_or_else(|e| {
                let backup = db_path.with_extension("mmdb.bak");
                tracing::warn!("Memory DB corrupted: {e}. Backing up to {backup:?}");
                let _ = std::fs::rename(&db_path, &backup);
                VectorDB::new(make_config()).expect("Failed to create memory DB")
            })
        } else {
            VectorDB::new(make_config()).context("Failed to create memory DB")?
        };

        Ok(Self {
            db,
            data_dir: data_dir.to_path_buf(),
        })
    }

    // ─── Write Operations ───────────────────────────────

    /// Save a new typed memory entry
    pub fn save(
        &self,
        name: &str,
        description: &str,
        content: &str,
        memory_type: MemoryType,
        source: &str,
        session_id: &str,
        tags: &[String],
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let embedding = text_to_vector(content, EMBEDDING_DIM);

        let tag_values: Vec<MetadataValue> = tags
            .iter()
            .map(|t| MetadataValue::String(t.clone()))
            .collect();

        let mut meta = Metadata::new();
        meta.insert("name", name.to_string());
        meta.insert("description", description.to_string());
        meta.insert("content", content.to_string());
        meta.insert("memory_type", memory_type.as_str().to_string());
        meta.insert("source", source.to_string());
        meta.insert("created_at", now.clone());
        meta.insert("updated_at", now);
        meta.insert("session_id", session_id.to_string());
        meta.insert("recall_count", 0i64);
        meta.insert("tags", MetadataValue::List(tag_values));

        self.db
            .insert(&id, &embedding, Some(meta))
            .context("Failed to save memory")?;

        tracing::info!("Memory saved: [{:?}] {name}", memory_type);
        Ok(id)
    }

    /// Update an existing memory's content and timestamp
    pub fn update(&self, id: &str, content: &str, description: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let embedding = text_to_vector(content, EMBEDDING_DIM);

        let current = self.db.get(id)?;
        if let Some((_, Some(mut meta))) = current {
            meta.insert("content", content.to_string());
            meta.insert("description", description.to_string());
            meta.insert("updated_at", now);
            self.db.update_document(id, Some(&embedding), Some(meta))?;
            tracing::info!("Memory updated: {id}");
        }
        Ok(())
    }

    /// Delete a memory by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        Ok(self.db.delete(id)?)
    }

    /// Increment recall count (tracks how useful a memory is)
    pub fn touch(&self, id: &str) -> Result<()> {
        let current = self.db.get(id)?;
        if let Some((_, Some(mut meta))) = current {
            let count = meta_int(&meta, "recall_count") + 1;
            meta.insert("recall_count", count);
            self.db.update_document(id, None, Some(meta))?;
        }
        Ok(())
    }

    // ─── Read Operations ────────────────────────────────

    /// Keyword search across all memories (BM25)
    pub fn keyword_search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let results = self.db.keyword_search(query, limit)?;
        Ok(results
            .into_iter()
            .filter_map(|r| r.metadata.as_ref().map(|m| meta_to_entry(&r.id, m)))
            .collect())
    }

    /// Vector similarity search
    pub fn vector_search(&self, query: &str, limit: usize) -> Result<Vec<MemoryMatch>> {
        let query_vec = text_to_vector(query, EMBEDDING_DIM);
        let results = self.db.search(&query_vec, limit)?;
        Ok(results
            .into_iter()
            .filter_map(|r| {
                let meta = self.db.get(&r.id).ok()??;
                let metadata = meta.1?;
                let entry = meta_to_entry(&r.id, &metadata);
                let age = super::retrieval::memory_age_days(&entry.updated_at);
                Some(MemoryMatch {
                    age_days: age,
                    freshness_warning: super::retrieval::freshness_text(age),
                    score: r.distance,
                    entry,
                })
            })
            .collect())
    }

    /// Filter by memory type
    pub fn by_type(&self, memory_type: MemoryType, limit: usize) -> Result<Vec<MemoryEntry>> {
        let filter = Filter::eq("memory_type", memory_type.as_str());
        let results = self.db.filter_search(filter, limit)?;
        Ok(results
            .into_iter()
            .filter_map(|r| r.metadata.as_ref().map(|m| meta_to_entry(&r.id, m)))
            .collect())
    }

    /// Get all memory headers (lightweight scan)
    pub fn scan_headers(&self) -> Result<Vec<MemoryHeader>> {
        let all_ids = self.db.list_ids()?;
        let mut headers = Vec::new();

        for id in all_ids {
            if let Ok(Some((_, Some(meta)))) = self.db.get(&id) {
                let mt_str = meta_str(&meta, "memory_type");
                let memory_type = MemoryType::from_str(&mt_str).unwrap_or(MemoryType::Project);
                headers.push(MemoryHeader {
                    id: id.to_string(),
                    name: meta_str(&meta, "name"),
                    description: meta_str(&meta, "description"),
                    memory_type,
                    updated_at: meta_str(&meta, "updated_at"),
                    recall_count: meta_int(&meta, "recall_count"),
                });
            }
        }

        Ok(headers)
    }

    /// Get a single memory by ID
    pub fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        match self.db.get(id)? {
            Some((_, Some(meta))) => Ok(Some(meta_to_entry(id, &meta))),
            _ => Ok(None),
        }
    }

    /// Total number of stored memories
    pub fn count(&self) -> usize {
        self.db.len()
    }

    // ─── Maintenance ────────────────────────────────────

    /// Persist to disk
    pub fn flush(&self) -> Result<()> {
        self.db.save(&self.data_dir.join("memory.mmdb"))?;
        Ok(())
    }

    /// Check if consolidation is needed
    pub fn needs_consolidation(&self) -> bool {
        self.db.len() > MAX_MEMORIES
    }

    /// Delete memories older than `days` that have low recall counts.
    /// Keeps highly-recalled memories regardless of age.
    pub fn expire_old_memories(&self, days: i64) -> Result<usize> {
        use super::retrieval::memory_age_days;
        let headers = self.scan_headers()?;
        let mut deleted = 0;

        for h in &headers {
            let age = memory_age_days(&h.updated_at);
            // Only expire old memories with low recall
            if age > days && h.recall_count < 3 && h.memory_type != MemoryType::User {
                if self.db.delete(&h.id)? {
                    deleted += 1;
                }
            }
        }

        if deleted > 0 {
            tracing::info!("Expired {deleted} stale memories (>{days} days old, low recall)");
        }
        Ok(deleted)
    }

    /// Consolidate: expire old entries, prune session memories, keep under MAX.
    pub fn consolidate(&self) -> Result<usize> {
        let mut total_cleaned = 0;

        // Phase 1: Expire old low-recall memories (>30 days)
        total_cleaned += self.expire_old_memories(30)?;

        // Phase 2: If still over limit, remove oldest session summaries
        if self.db.len() > MAX_MEMORIES {
            let sessions = self.by_type(MemoryType::Session, self.db.len())?;
            let mut sorted = sessions;
            sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at)); // oldest first

            let to_remove = sorted.len().saturating_sub(20); // Keep latest 20 sessions
            for entry in sorted.into_iter().take(to_remove) {
                if self.db.delete(&entry.id)? {
                    total_cleaned += 1;
                }
            }
        }

        // Phase 3: If still over, remove lowest-recall memories
        if self.db.len() > MAX_MEMORIES {
            let mut headers = self.scan_headers()?;
            headers.sort_by_key(|h| h.recall_count); // lowest recall first
            let excess = self.db.len() - MAX_MEMORIES;
            for h in headers.into_iter().take(excess) {
                if self.db.delete(&h.id)? {
                    total_cleaned += 1;
                }
            }
        }

        if total_cleaned > 0 {
            tracing::info!("Consolidated memory: removed {total_cleaned} entries, {} remaining", self.db.len());
        }
        Ok(total_cleaned)
    }

    /// Load CLAUDE.md from project directory and index its content
    pub fn bootstrap_from_claude_md(&self, cwd: &Path, session_id: &str) -> Result<usize> {
        let candidates = [
            "CLAUDE.md",
            ".claude/CLAUDE.md",
            ".claude/memory.md",
            ".claude/MEMORY.md",
        ];

        let mut count = 0;
        for name in &candidates {
            let path = cwd.join(name);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Check if we already indexed this file
                    let existing = self.keyword_search(name, 1)?;
                    if existing.is_empty() {
                        self.save(
                            name,
                            &format!("Project instructions from {name}"),
                            &content,
                            MemoryType::Project,
                            "claude.md",
                            session_id,
                            &["project-instructions".to_string()],
                        )?;
                        count += 1;
                        tracing::info!("Indexed {name} into memory store");
                    }
                }
            }
        }
        Ok(count)
    }
}

// ─── Helpers ────────────────────────────────────────────

fn meta_to_entry(id: &str, meta: &Metadata) -> MemoryEntry {
    let mt_str = meta_str(meta, "memory_type");
    let tags = match meta.get("tags") {
        Some(MetadataValue::List(list)) => list
            .iter()
            .filter_map(|v| match v {
                MetadataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    MemoryEntry {
        id: id.to_string(),
        name: meta_str(meta, "name"),
        description: meta_str(meta, "description"),
        content: meta_str(meta, "content"),
        memory_type: MemoryType::from_str(&mt_str).unwrap_or(MemoryType::Project),
        source: meta_str(meta, "source"),
        created_at: meta_str(meta, "created_at"),
        updated_at: meta_str(meta, "updated_at"),
        tags,
        session_id: meta_str(meta, "session_id"),
        recall_count: meta_int(meta, "recall_count"),
    }
}

// Freshness functions now in retrieval.rs (DRY)

/// Deterministic vector from text (hash-based).
/// For production, replace with local embeddings (Candle) or API embeddings.
fn text_to_vector(text: &str, dim: usize) -> Vec<f32> {
    use sha2::{Digest, Sha256};
    let mut vector = vec![0.0f32; dim];
    let hash = Sha256::digest(text.as_bytes());
    for (i, byte) in hash.iter().enumerate() {
        if i < dim {
            vector[i] = (*byte as f32 / 255.0) * 2.0 - 1.0;
        }
    }
    if dim > 32 {
        for i in 32..dim {
            vector[i] = vector[i % 32] * 0.5 + vector[(i + 7) % 32] * 0.5;
        }
    }
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vector {
            *v /= norm;
        }
    }
    vector
}
