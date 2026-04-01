use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use super::file_read::safe_resolve_path;
use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Perform exact string replacement in a file. Replaces old_string with new_string."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
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

        let old_string = input
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_string' parameter"))?;

        let new_string = input
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_string' parameter"))?;

        let replace_all = input
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Validate path (prevents directory traversal)
        let path = match safe_resolve_path(file_path) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(e)),
        };

        let content = std::fs::read_to_string(&path)?;

        let match_count = content.matches(old_string).count();
        if match_count == 0 {
            return Ok(ToolOutput::error(format!(
                "old_string not found in {file_path}. Make sure the string matches exactly."
            )));
        }

        if !replace_all && match_count > 1 {
            return Ok(ToolOutput::error(format!(
                "old_string found {match_count} times in {file_path}. Use replace_all: true or provide a more specific string."
            )));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        std::fs::write(&path, &new_content)?;

        let replacements = if replace_all {
            format!("{match_count} replacements")
        } else {
            "1 replacement".to_string()
        };

        Ok(ToolOutput::success(format!(
            "Edited {file_path} ({replacements})"
        )))
    }
}
