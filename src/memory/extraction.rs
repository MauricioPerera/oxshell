use anyhow::Result;

use super::store::MemoryStore;
use super::types::MemoryType;
use crate::llm::types::Message;

/// Background memory extraction — analyzes conversation messages
/// and extracts durable memories worth persisting.
///
/// Equivalent to KAIROS's `extractMemories` service but runs locally
/// with heuristic-based extraction instead of a forked Sonnet agent.
///
/// Extraction rules:
/// - User corrections ("don't", "stop", "no not that") → Feedback memory
/// - User confirmations ("yes exactly", "perfect") → Feedback memory
/// - User profile info ("I'm a", "I work on") → User memory
/// - Project decisions ("we decided", "deadline is") → Project memory
/// - External references ("check Linear", "Slack channel") → Reference memory
pub struct MemoryExtractor<'a> {
    store: &'a MemoryStore,
    session_id: String,
    /// Cursor: index of last processed message
    cursor: usize,
}

impl<'a> MemoryExtractor<'a> {
    pub fn new(store: &'a MemoryStore, session_id: &str) -> Self {
        Self {
            store,
            session_id: session_id.to_string(),
            cursor: 0,
        }
    }

    /// Process new messages since last extraction.
    /// Returns the number of memories extracted.
    pub fn extract_from_messages(&mut self, messages: &[Message]) -> Result<usize> {
        if messages.len() <= self.cursor {
            return Ok(0);
        }

        let new_messages = &messages[self.cursor..];
        let mut extracted = 0;

        for msg in new_messages {
            let content = msg.content.as_deref().unwrap_or("");
            if content.is_empty() {
                continue;
            }

            // Only extract from user messages (like KAIROS)
            if msg.role != crate::llm::types::Role::User {
                continue;
            }

            // Skip tool results and system messages
            if content.starts_with("[Tool") || content.starts_with("[Calling") {
                continue;
            }

            // Skip messages containing secrets (never persist these)
            if contains_secret(content) {
                tracing::debug!("Skipping memory extraction: message contains potential secret");
                continue;
            }

            // Try each extraction pattern
            if let Some(mem) = extract_feedback(content) {
                self.store.save(
                    &mem.name,
                    &mem.description,
                    &mem.content,
                    MemoryType::Feedback,
                    "extraction",
                    &self.session_id,
                    &["auto-extracted".to_string()],
                )?;
                extracted += 1;
            }

            if let Some(mem) = extract_user_profile(content) {
                self.store.save(
                    &mem.name,
                    &mem.description,
                    &mem.content,
                    MemoryType::User,
                    "extraction",
                    &self.session_id,
                    &["auto-extracted".to_string()],
                )?;
                extracted += 1;
            }

            if let Some(mem) = extract_project_decision(content) {
                self.store.save(
                    &mem.name,
                    &mem.description,
                    &mem.content,
                    MemoryType::Project,
                    "extraction",
                    &self.session_id,
                    &["auto-extracted".to_string()],
                )?;
                extracted += 1;
            }

            if let Some(mem) = extract_reference(content) {
                self.store.save(
                    &mem.name,
                    &mem.description,
                    &mem.content,
                    MemoryType::Reference,
                    "extraction",
                    &self.session_id,
                    &["auto-extracted".to_string()],
                )?;
                extracted += 1;
            }
        }

        // Advance cursor past processed messages
        self.cursor = messages.len();

        if extracted > 0 {
            tracing::info!("Extracted {extracted} memories from conversation");
        }

        Ok(extracted)
    }

    /// Generate session summary memory (like KAIROS SessionMemory)
    pub fn save_session_summary(&self, messages: &[Message]) -> Result<()> {
        if messages.len() < 4 {
            return Ok(()); // Too short to summarize
        }

        // Collect user messages as session context
        let user_messages: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == crate::llm::types::Role::User)
            .filter_map(|m| m.content.as_deref())
            .filter(|c| !c.starts_with("[Tool"))
            .take(10)
            .collect();

        if user_messages.is_empty() {
            return Ok(());
        }

        let summary = format!(
            "Session topics: {}",
            user_messages
                .iter()
                .map(|m| {
                    let truncated: String = m.chars().take(80).collect();
                    if m.len() > 80 {
                        format!("{truncated}...")
                    } else {
                        truncated
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        );

        self.store.save(
            &format!("Session {}", &self.session_id[..8]),
            "Auto-generated session summary",
            &summary,
            MemoryType::Session,
            "session",
            &self.session_id,
            &["session-summary".to_string()],
        )?;

        Ok(())
    }
}

// ─── Secret Detection ───────────────────────────────────

/// Check if text contains potential secrets (API keys, passwords, tokens)
fn contains_secret(text: &str) -> bool {
    let secret_patterns = [
        "sk_live_", "sk_test_", "sk-ant-", "sk-proj-",
        "ghp_", "gho_", "github_pat_",
        "xoxb-", "xoxp-", // Slack
        "AKIA",  // AWS access key
        "password=", "passwd=", "secret=",
        "bearer ", "authorization:",
        "-----BEGIN RSA", "-----BEGIN PRIVATE",
        "eyJ",  // JWT token prefix (base64 of {"a)
    ];
    let lower = text.to_lowercase();
    secret_patterns.iter().any(|p| lower.contains(&p.to_lowercase()))
}

// ─── Pattern Extractors ─────────────────────────────────

struct ExtractedMemory {
    name: String,
    description: String,
    content: String,
}

/// Detect user corrections or confirmations.
/// Requires first-person context ("I", "you", "please") to avoid false positives from tool output.
fn extract_feedback(text: &str) -> Option<ExtractedMemory> {
    let lower = text.to_lowercase();

    // Must contain first-person context to avoid matching tool error output
    let has_context = lower.starts_with("i ") || lower.starts_with("you ")
        || lower.starts_with("please") || lower.starts_with("don't")
        || lower.starts_with("stop") || lower.starts_with("no,")
        || lower.starts_with("no ");

    if !has_context {
        return None;
    }

    let patterns = [
        ("don't ", "User correction"),
        ("do not ", "User correction"),
        ("stop doing", "User correction"),
        ("no not that", "User correction"),
        ("never do", "User correction"),
        ("instead of", "User preference"),
        ("i prefer", "User preference"),
        ("always use", "User preference"),
        ("yes exactly", "User confirmation"),
        ("perfect, keep", "User confirmation"),
        ("that's right", "User confirmation"),
    ];

    for (pattern, desc) in &patterns {
        if lower.contains(pattern) && text.len() > 10 && text.len() < 500 {
            return Some(ExtractedMemory {
                name: format!("Feedback: {}", &text[..text.len().min(50)]),
                description: desc.to_string(),
                content: text.to_string(),
            });
        }
    }
    None
}

/// Detect user profile information
fn extract_user_profile(text: &str) -> Option<ExtractedMemory> {
    let lower = text.to_lowercase();
    let patterns = [
        "i'm a ",
        "i am a ",
        "i work on",
        "i work as",
        "my role is",
        "i specialize in",
        "i've been",
        "my background",
        "i'm new to",
        "i'm experienced",
    ];

    for pattern in &patterns {
        if lower.contains(pattern) && text.len() > 10 && text.len() < 500 {
            return Some(ExtractedMemory {
                name: format!("Profile: {}", &text[..text.len().min(50)]),
                description: "User profile information".to_string(),
                content: text.to_string(),
            });
        }
    }
    None
}

/// Detect project decisions and deadlines
fn extract_project_decision(text: &str) -> Option<ExtractedMemory> {
    let lower = text.to_lowercase();
    let patterns = [
        "we decided",
        "the deadline",
        "we're using",
        "the plan is",
        "we need to",
        "the reason we",
        "merge freeze",
        "release branch",
        "sprint goal",
    ];

    for pattern in &patterns {
        if lower.contains(pattern) && text.len() > 15 && text.len() < 500 {
            return Some(ExtractedMemory {
                name: format!("Decision: {}", &text[..text.len().min(50)]),
                description: "Project decision or deadline".to_string(),
                content: text.to_string(),
            });
        }
    }
    None
}

/// Detect references to external systems
fn extract_reference(text: &str) -> Option<ExtractedMemory> {
    let lower = text.to_lowercase();
    let patterns = [
        ("linear", "Linear project reference"),
        ("jira", "Jira reference"),
        ("slack channel", "Slack channel reference"),
        ("grafana", "Grafana dashboard reference"),
        ("confluence", "Confluence page reference"),
        ("notion", "Notion reference"),
        ("github.com/", "GitHub repository reference"),
        ("figma", "Figma design reference"),
    ];

    for (pattern, desc) in &patterns {
        if lower.contains(pattern) && text.len() > 10 && text.len() < 500 {
            return Some(ExtractedMemory {
                name: format!("Ref: {}", &text[..text.len().min(50)]),
                description: desc.to_string(),
                content: text.to_string(),
            });
        }
    }
    None
}
