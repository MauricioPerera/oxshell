use anyhow::Result;
use std::path::{Path, PathBuf};

use super::store::MemoryStore;
use super::types::MemoryHeader;

const MAX_INDEX_LINES: usize = 200;
const MAX_INDEX_BYTES: usize = 25_000;

/// Manages the MEMORY.md index file — a human-readable table of contents
/// for all stored memories. Kept in sync with the minimemory DB.
///
/// Format:
/// ```
/// - [Title](type) — one-line description
/// ```
pub struct MemoryIndex {
    path: PathBuf,
}

impl MemoryIndex {
    pub fn new(cwd: &Path) -> Self {
        let memory_dir = cwd.join(".oxshell");
        let _ = std::fs::create_dir_all(&memory_dir);
        Self {
            path: memory_dir.join("MEMORY.md"),
        }
    }

    /// Rebuild MEMORY.md from the current state of the memory store
    pub fn rebuild(&self, store: &MemoryStore) -> Result<()> {
        let headers = store.scan_headers()?;
        let content = self.format_index(&headers);
        std::fs::write(&self.path, content)?;
        tracing::info!("MEMORY.md rebuilt with {} entries", headers.len());
        Ok(())
    }

    /// Read the current MEMORY.md content (for system prompt injection)
    pub fn load(&self) -> Option<String> {
        if self.path.exists() {
            std::fs::read_to_string(&self.path).ok()
        } else {
            None
        }
    }

    /// Format headers into MEMORY.md content, respecting size limits.
    /// Within each type group, sorted by recency (most recent first).
    fn format_index(&self, headers: &[MemoryHeader]) -> String {
        let mut lines = Vec::new();
        let mut total_bytes = 0;

        // Group by type, sorted by recency within each group
        let type_order = ["user", "feedback", "project", "reference", "session"];

        for type_name in &type_order {
            let mut typed: Vec<&MemoryHeader> = headers
                .iter()
                .filter(|h| h.memory_type.as_str() == *type_name)
                .collect();
            // Sort by recency (most recently updated first)
            typed.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            if typed.is_empty() {
                continue;
            }

            let header_line = format!("\n## {}\n", capitalize(type_name));
            total_bytes += header_line.len();
            if total_bytes > MAX_INDEX_BYTES || lines.len() >= MAX_INDEX_LINES {
                break;
            }
            lines.push(header_line);

            for h in &typed {
                let line = if h.description.is_empty() {
                    format!("- **{}**\n", h.name)
                } else {
                    format!("- **{}** — {}\n", h.name, h.description)
                };

                total_bytes += line.len();
                if total_bytes > MAX_INDEX_BYTES || lines.len() >= MAX_INDEX_LINES {
                    lines.push(format!(
                        "\n> MEMORY.md truncated at {} entries. Total: {}\n",
                        lines.len(),
                        headers.len()
                    ));
                    return lines.join("");
                }
                lines.push(line);
            }
        }

        if lines.is_empty() {
            return "# Memory\n\nNo memories stored yet.\n".to_string();
        }

        let mut output = format!("# Memory ({} entries)\n", headers.len());
        output.push_str(&lines.join(""));
        output
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
