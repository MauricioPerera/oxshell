use anyhow::Result;

use super::store::MemoryStore;
use super::types::*;

const MAX_RELEVANT: usize = 5;
const MAX_CONTENT_BYTES: usize = 4096;

/// Hybrid retrieval system — replaces KAIROS's Sonnet-based selection
/// with local BM25 + vector search at zero API cost.
pub struct MemoryRetriever<'a> {
    store: &'a MemoryStore,
}

impl<'a> MemoryRetriever<'a> {
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Find memories relevant to the current query.
    /// Uses hybrid search: BM25 keyword + vector similarity, deduped via RRF.
    /// Fresh instance per query — no session accumulation issues.
    pub fn find_relevant(&self, query: &str) -> Result<Vec<MemoryMatch>> {
        // BM25 keyword search
        let keyword_results = self.store.keyword_search(query, MAX_RELEVANT * 2)?;

        // Vector similarity search
        let vector_results = self.store.vector_search(query, MAX_RELEVANT * 2)?;

        // Merge and deduplicate (Reciprocal Rank Fusion)
        let mut scored: std::collections::HashMap<String, ScoredMemory> =
            std::collections::HashMap::new();

        for (rank, entry) in keyword_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (60.0 + rank as f64);
            scored
                .entry(entry.id.clone())
                .and_modify(|s| s.rrf_score += rrf_score)
                .or_insert(ScoredMemory {
                    entry,
                    rrf_score,
                });
        }

        for (rank, mmatch) in vector_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (60.0 + rank as f64);
            scored
                .entry(mmatch.entry.id.clone())
                .and_modify(|s| s.rrf_score += rrf_score)
                .or_insert(ScoredMemory {
                    entry: mmatch.entry,
                    rrf_score,
                });
        }

        let mut ranked: Vec<ScoredMemory> = scored.into_values().collect();
        ranked.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut results = Vec::new();
        for sm in ranked.into_iter().take(MAX_RELEVANT) {
            let mut entry = sm.entry;

            // Truncate at sentence boundary within 4KB limit
            if entry.content.len() > MAX_CONTENT_BYTES {
                let truncated = &entry.content[..MAX_CONTENT_BYTES];
                // Find last sentence end within limit
                let cut_point = truncated
                    .rfind(". ")
                    .or_else(|| truncated.rfind(".\n"))
                    .or_else(|| truncated.rfind('\n'))
                    .unwrap_or(MAX_CONTENT_BYTES);
                entry.content.truncate(cut_point + 1);
                entry.content.push_str("\n... (truncated)");
            }

            let _ = self.store.touch(&entry.id);

            let age = memory_age_days(&entry.updated_at);
            let warning = freshness_text(age);

            results.push(MemoryMatch {
                score: sm.rrf_score as f32,
                age_days: age,
                freshness_warning: warning,
                entry,
            });
        }

        Ok(results)
    }

    /// Format relevant memories for injection into system prompt
    pub fn format_for_prompt(&self, query: &str) -> Result<String> {
        let matches = self.find_relevant(query)?;

        if matches.is_empty() {
            return Ok(String::new());
        }

        let mut sections = Vec::new();
        for m in &matches {
            let mut section = format!(
                "### {} [{}]\n",
                m.entry.name,
                m.entry.memory_type.as_str()
            );
            if !m.freshness_warning.is_empty() {
                section.push_str(&format!("> {}\n", m.freshness_warning));
            }
            section.push_str(&m.entry.content);
            sections.push(section);
        }

        Ok(format!(
            "# Relevant Memories ({} found)\n\n{}",
            matches.len(),
            sections.join("\n\n")
        ))
    }
}

struct ScoredMemory {
    entry: MemoryEntry,
    rrf_score: f64,
}

/// Shared freshness logic (DRY — used by both retrieval and store)
pub fn memory_age_days(updated_at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(updated_at)
        .map(|dt| (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_days())
        .unwrap_or(0)
}

pub fn freshness_text(age_days: i64) -> String {
    if age_days > 7 {
        format!("WARNING: Memory is {age_days} days old. Verify before acting.")
    } else if age_days > 1 {
        format!("Note: Memory is {age_days} days old.")
    } else {
        String::new()
    }
}
