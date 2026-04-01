use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::llm::types::Message;

/// A single entry in a session JSONL file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub timestamp: DateTime<Utc>,
    pub message: Message,
    /// True if this message is a compaction summary (not original conversation)
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_compaction_summary: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

/// Lightweight metadata for the session index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub model: String,
    pub cwd: String,
}
