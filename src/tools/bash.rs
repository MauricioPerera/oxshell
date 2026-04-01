use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

pub struct BashTool;

/// Commands that are blocked for safety
const BLOCKED_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs.",
    "dd if=",
    ":(){:|:&};:",
    "> /dev/sda",
    "chmod -R 777 /",
    "curl|sh",
    "curl|bash",
    "wget|sh",
    "wget|bash",
    "shutdown",
    "reboot",
    "format c:",
    "del /f /s /q c:",
    "netsh advfirewall",
    "reg delete",
];

/// Check if a command matches any blocked pattern.
/// Also detects common evasion techniques.
fn is_blocked(command: &str) -> Option<&'static str> {
    // Normalize: lowercase, strip spaces, handle common encodings
    let lower = command.to_lowercase();
    let compact = lower.replace(' ', "");

    // Check direct patterns
    for pattern in BLOCKED_PATTERNS {
        let pattern_compact = pattern.to_lowercase().replace(' ', "");
        if compact.contains(&pattern_compact) {
            return Some(pattern);
        }
    }

    // Detect shell injection via $(), backticks, or eval
    if lower.contains("$(") && (compact.contains("rm-rf") || compact.contains("mkfs")) {
        return Some("shell expansion with dangerous command");
    }
    if lower.contains("eval ") || lower.contains("eval\t") {
        return Some("eval (arbitrary code execution)");
    }
    if lower.contains("\\x") && lower.contains("rm") {
        return Some("hex-encoded dangerous command");
    }

    None
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Dangerous commands are blocked."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (max 120000)",
                    "default": 120000
                }
            },
            "required": ["command"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;

        // Block dangerous commands
        if let Some(pattern) = is_blocked(command) {
            return Ok(ToolOutput::error(format!(
                "Blocked: command matches dangerous pattern '{pattern}'"
            )));
        }

        let timeout_ms = input
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000)
            .min(120_000);

        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("bash", "-c")
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            Command::new(shell).arg(flag).arg(command).output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut result_text = String::new();

                if !stdout.is_empty() {
                    result_text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result_text.is_empty() {
                        result_text.push('\n');
                    }
                    result_text.push_str(&format!("stderr: {stderr}"));
                }

                if exit_code != 0 {
                    result_text.push_str(&format!("\n(exit code: {exit_code})"));
                }

                if result_text.is_empty() {
                    result_text = "(no output)".to_string();
                }

                // Truncate very long outputs
                if result_text.len() > 100_000 {
                    result_text.truncate(100_000);
                    result_text.push_str("\n... (output truncated)");
                }

                Ok(ToolOutput::success(result_text))
            }
            Ok(Err(e)) => Ok(ToolOutput::error(format!("Failed to execute command: {e}"))),
            Err(_) => Ok(ToolOutput::error(format!(
                "Command timed out after {timeout_ms}ms"
            ))),
        }
    }
}
