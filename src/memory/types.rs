use serde::{Deserialize, Serialize};

/// Memory types following KAIROS taxonomy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    /// User profile: role, goals, preferences, knowledge level
    User,
    /// Feedback: corrections AND confirmations on approach
    Feedback,
    /// Project: ongoing work, deadlines, decisions, bugs
    Project,
    /// Reference: pointers to external systems (Linear, Slack, Grafana)
    Reference,
    /// Session: auto-generated session notes (files touched, commands run)
    Session,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Feedback => "feedback",
            Self::Project => "project",
            Self::Reference => "reference",
            Self::Session => "session",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "user" => Some(Self::User),
            "feedback" => Some(Self::Feedback),
            "project" => Some(Self::Project),
            "reference" => Some(Self::Reference),
            "session" => Some(Self::Session),
            _ => None,
        }
    }
}

/// A typed memory entry stored in minimemory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    /// One-line title
    pub name: String,
    /// One-line description (used for relevance matching)
    pub description: String,
    /// Full content
    pub content: String,
    /// Memory type
    pub memory_type: MemoryType,
    /// Source: "auto", "user", "claude.md", "extraction"
    pub source: String,
    /// ISO 8601 timestamp
    pub created_at: String,
    /// ISO 8601 timestamp
    pub updated_at: String,
    /// Free-form tags
    pub tags: Vec<String>,
    /// Session ID that created this memory
    pub session_id: String,
    /// How many times this memory was recalled
    pub recall_count: i64,
}

/// Lightweight header for scanning without loading full content
#[derive(Debug, Clone)]
pub struct MemoryHeader {
    pub id: String,
    pub name: String,
    pub description: String,
    pub memory_type: MemoryType,
    pub updated_at: String,
    pub recall_count: i64,
}

/// Result of a memory search with relevance score
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MemoryMatch {
    pub entry: MemoryEntry,
    pub score: f32,
    /// Days since last update
    pub age_days: i64,
    /// Freshness warning (empty if <1 day old)
    pub freshness_warning: String,
}
