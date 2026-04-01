use anyhow::{Context, Result};
use chrono::Utc;
use minimemory::{Config, Distance, Metadata, MetadataValue, VectorDB};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const EMBEDDING_DIM: usize = 64;

/// Conversation history storage (separate from the memory system).
/// Stores raw messages for session persistence and search.
pub struct ConversationStore {
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

impl ConversationStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("conversations.mmdb");

        let db = if db_path.exists() {
            VectorDB::open(&db_path).unwrap_or_else(|e| {
                let backup = db_path.with_extension("mmdb.bak");
                tracing::warn!("Conversations DB corrupted: {e}. Backing up to {backup:?}");
                let _ = std::fs::rename(&db_path, &backup);
                VectorDB::new(make_config()).expect("Failed to create conversations DB")
            })
        } else {
            VectorDB::new(make_config()).context("Failed to create conversations DB")?
        };

        Ok(Self {
            db,
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn save_message(&self, session_id: &str, role: &str, content: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let embedding = text_to_vector(content, EMBEDDING_DIM);

        let mut meta = Metadata::new();
        meta.insert("session_id", session_id.to_string());
        meta.insert("role", role.to_string());
        meta.insert("content", content.to_string());
        meta.insert("timestamp", now);

        self.db
            .insert(&id, &embedding, Some(meta))
            .context("Failed to save message")?;

        Ok(id)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, String, String)>> {
        let results = self.db.keyword_search(query, limit)?;
        Ok(results
            .into_iter()
            .filter_map(|r| {
                r.metadata.as_ref().map(|m| {
                    (
                        meta_str(m, "role"),
                        meta_str(m, "content"),
                        meta_str(m, "timestamp"),
                    )
                })
            })
            .collect())
    }

    pub fn flush(&self) -> Result<()> {
        self.db
            .save(&self.data_dir.join("conversations.mmdb"))?;
        Ok(())
    }
}

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
