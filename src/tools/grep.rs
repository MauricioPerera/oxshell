use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use serde_json::{Value, json};
use std::path::Path;

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

pub struct GrepTool;

/// Check if content is likely binary (contains null bytes in first 8KB)
fn is_binary(content: &[u8]) -> bool {
    let check_len = content.len().min(8192);
    content[..check_len].contains(&0u8)
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regex patterns. Respects .gitignore. Returns matching lines with paths and line numbers."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to current directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs', '*.{ts,tsx}')"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case insensitive search",
                    "default": false
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of matches to return",
                    "default": 250
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

        let search_path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let file_glob = input.get("glob").and_then(|v| v.as_str());
        let case_insensitive = input
            .get("case_insensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(250) as usize;

        let path = Path::new(search_path);
        if !path.exists() {
            return Ok(ToolOutput::error(format!("Path not found: {search_path}")));
        }

        // Build regex
        let regex_pattern = if case_insensitive {
            format!("(?i){pattern}")
        } else {
            pattern.to_string()
        };

        let regex = match regex::Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid regex: {e}"))),
        };

        // Build glob matcher for file filtering
        let glob_matcher = file_glob.and_then(|g| {
            globset::GlobBuilder::new(g)
                .literal_separator(false)
                .build()
                .ok()
                .map(|gb| gb.compile_matcher())
        });

        let mut results = Vec::new();

        if path.is_file() {
            // Search single file
            search_file(path, path, &regex, &mut results, max_results);
        } else {
            // Use `ignore` crate — respects .gitignore
            let walker = WalkBuilder::new(path)
                .hidden(true)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .max_depth(Some(20))
                .build();

            for entry in walker.flatten() {
                if results.len() >= max_results {
                    break;
                }

                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    continue;
                }

                // Apply glob filter
                if let Some(ref matcher) = glob_matcher {
                    let rel = entry
                        .path()
                        .strip_prefix(path)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .replace('\\', "/");
                    if !matcher.is_match(&rel) {
                        continue;
                    }
                }

                search_file(entry.path(), path, &regex, &mut results, max_results);
            }
        }

        if results.is_empty() {
            return Ok(ToolOutput::success(format!(
                "No matches found for pattern: {pattern}"
            )));
        }

        let total = results.len();
        let output = results.join("\n");
        let footer = if total >= max_results {
            format!("\n... (limited to {max_results} results)")
        } else {
            format!("\n({total} matches)")
        };

        Ok(ToolOutput::success(format!("{output}{footer}")))
    }
}

fn search_file(
    file: &Path,
    root: &Path,
    regex: &regex::Regex,
    results: &mut Vec<String>,
    max_results: usize,
) {
    // Read file bytes first for binary check
    let content = match std::fs::read(file) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Skip binary files (check first 8KB for null bytes)
    if is_binary(&content) {
        return;
    }

    let text = match String::from_utf8(content) {
        Ok(t) => t,
        Err(_) => return,
    };

    let rel_path = file
        .strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");

    for (line_num, line) in text.lines().enumerate() {
        if results.len() >= max_results {
            break;
        }
        if regex.is_match(line) {
            results.push(format!("{}:{}:{}", rel_path, line_num + 1, line));
        }
    }
}
