use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;

use super::{Tool, ToolOutput};
use crate::permissions::{ToolPermission, is_sensitive_path};

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, or overwrites if it does."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let file_path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;

        let path = Path::new(file_path);
        let existed = path.exists(); // Check BEFORE write (TOCTOU fix)

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // For existing files, canonicalize and validate fully
        if path.exists() {
            let canonical = path.canonicalize()?;
            let canonical_str = canonical.to_string_lossy().to_lowercase();
            if is_sensitive_path(&canonical_str) {
                return Ok(ToolOutput::error(
                    "Access denied: cannot write to sensitive path".to_string(),
                ));
            }
            std::fs::write(&canonical, content)?;
        } else {
            // For new files: canonicalize parent + validate filename
            let parent = path.parent().unwrap_or(Path::new("."));
            let canonical_parent = parent.canonicalize()?;
            let file_name = path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;

            // Check that filename doesn't contain path separators (prevent traversal)
            let name_str = file_name.to_string_lossy();
            if name_str.contains('/') || name_str.contains('\\') || name_str.contains("..") {
                return Ok(ToolOutput::error(
                    "Invalid file name: contains path separators".to_string(),
                ));
            }

            let final_path = canonical_parent.join(file_name);

            // Validate final resolved path
            let final_str = final_path.to_string_lossy().to_lowercase();
            if is_sensitive_path(&final_str) {
                return Ok(ToolOutput::error(
                    "Access denied: cannot write to sensitive path".to_string(),
                ));
            }

            std::fs::write(&final_path, content)?;
        }

        let line_count = content.lines().count();
        let byte_count = content.len();
        let action = if existed { "Updated" } else { "Created" };
        Ok(ToolOutput::success(format!(
            "{action} {file_path} ({line_count} lines, {byte_count} bytes)"
        )))
    }
}

// Sensitive path check: uses crate::permissions::is_sensitive_path (unified list)
