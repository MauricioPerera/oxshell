use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use serde_json::{Value, json};
use std::path::Path;

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Respects .gitignore. Returns paths sorted by modification time."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g. '**/*.rs', 'src/**/*.ts')"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in (defaults to current directory)"
                }
            },
            "required": ["pattern"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::AutoApprove
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;

        let base_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let base = Path::new(base_path);
        if !base.exists() {
            return Ok(ToolOutput::error(format!(
                "Directory not found: {base_path}"
            )));
        }

        let glob = match globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
        {
            Ok(g) => g.compile_matcher(),
            Err(e) => return Ok(ToolOutput::error(format!("Invalid glob pattern: {e}"))),
        };

        let mut matches: Vec<(String, std::time::SystemTime)> = Vec::new();

        // Use `ignore` crate — respects .gitignore, skips hidden/binary by default
        let walker = WalkBuilder::new(base)
            .hidden(true)       // Skip hidden files
            .git_ignore(true)   // Respect .gitignore
            .git_global(true)   // Respect global gitignore
            .git_exclude(true)  // Respect .git/info/exclude
            .max_depth(Some(20))
            .build();

        for entry in walker.flatten() {
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }

            if let Ok(rel) = entry.path().strip_prefix(base) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if glob.is_match(&rel_str) {
                    let mtime = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    matches.push((rel_str, mtime));
                }
            }
        }

        // Sort by modification time (newest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        let total = matches.len();
        let display: Vec<&str> = matches.iter().take(1000).map(|(p, _)| p.as_str()).collect();

        let mut output = display.join("\n");
        if total > 1000 {
            output.push_str(&format!("\n... ({total} total, showing first 1000)"));
        } else if total == 0 {
            output = format!("No files matching pattern: {pattern}");
        } else {
            output.push_str(&format!("\n({total} matches)"));
        }

        Ok(ToolOutput::success(output))
    }
}
