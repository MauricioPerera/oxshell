use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

pub struct FileReadTool;

const MAX_LINES: usize = 2000;

/// Resolve and validate a file path, preventing directory traversal.
/// Returns the canonicalized path if safe, or an error message.
pub fn safe_resolve_path(file_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(file_path);

    // If relative, resolve against cwd
    let absolute = if path.is_relative() {
        std::env::current_dir()
            .map_err(|e| format!("Cannot get cwd: {e}"))?
            .join(path)
    } else {
        path.to_path_buf()
    };

    // Canonicalize to resolve symlinks and ..
    let canonical = absolute
        .canonicalize()
        .map_err(|_| format!("Path not found or inaccessible: {file_path}"))?;

    // Block known sensitive paths
    let canonical_str = canonical.to_string_lossy().to_lowercase();
    let blocked = [
        "/etc/shadow", "/etc/passwd", "/.ssh/", "/credentials",
        "/.env", "/secrets", "/.aws/", "/.gnupg/",
    ];
    for pattern in &blocked {
        if canonical_str.contains(pattern) {
            return Err(format!("Access denied: path matches blocked pattern '{pattern}'"));
        }
    }

    Ok(canonical)
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content with line numbers."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["file_path"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::AutoApprove
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let file_path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let offset = input
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1) as usize;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(MAX_LINES as u64) as usize;

        // Validate path (prevents directory traversal)
        let path = match safe_resolve_path(file_path) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(e)),
        };

        if !path.is_file() {
            return Ok(ToolOutput::error(format!(
                "Not a file (is a directory?): {file_path}"
            )));
        }

        // Read file and check size after (avoids TOCTOU race)
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {e}"))?;

        if content.len() > 10 * 1024 * 1024 {
            return Ok(ToolOutput::error(
                "File too large (>10MB). Use offset/limit to read portions.".to_string(),
            ));
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = (offset - 1).min(total_lines);
        let end = (start + limit).min(total_lines);

        let mut output = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            let line_num = start + i + 1;
            output.push_str(&format!("{line_num}\t{line}\n"));
        }

        if end < total_lines {
            output.push_str(&format!(
                "\n... ({} more lines, {} total)",
                total_lines - end,
                total_lines
            ));
        }

        if output.is_empty() {
            output = "(empty file)".to_string();
        }

        Ok(ToolOutput::success(output))
    }
}
