use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

const MAX_RESPONSE_BYTES: usize = 8192;

/// A2E (Agent-to-Execution) tool.
/// Sends a declarative JSONL workflow to an A2E server for execution.
pub struct A2ETool {
    server_url: String,
    api_key: String,
}

impl A2ETool {
    pub fn new(server_url: String, api_key: String) -> Self {
        // Warn if not HTTPS
        if !server_url.starts_with("https://") && !server_url.starts_with("http://localhost") && !server_url.starts_with("http://127.0.0.1") {
            tracing::warn!("A2E server URL is not HTTPS — credentials may be transmitted insecurely");
        }
        Self { server_url, api_key }
    }

    pub fn from_env() -> Option<Self> {
        let server_url = std::env::var("A2E_SERVER_URL").ok()?;
        let api_key = std::env::var("A2E_API_KEY").ok().unwrap_or_default();
        Some(Self::new(server_url, api_key))
    }
}

#[async_trait]
impl Tool for A2ETool {
    fn name(&self) -> &str {
        "a2e_execute"
    }

    fn description(&self) -> &str {
        "Execute a declarative workflow via A2E. Send JSONL with operationUpdate and beginExecution. \
         Operations: ApiCall, FilterData, TransformData, Conditional, Loop, StoreData, MergeData."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "description": "JSONL workflow. Each line is a JSON object with operationUpdate or beginExecution."
                },
                "validate_only": {
                    "type": "boolean",
                    "description": "If true, only validate without executing",
                    "default": false
                }
            },
            "required": ["workflow"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let workflow = input
            .get("workflow")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'workflow' parameter"))?;

        let validate_only = input
            .get("validate_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let endpoint = if validate_only {
            format!("{}/validate", self.server_url)
        } else {
            format!("{}/execute", self.server_url)
        };

        let client = reqwest::Client::new();
        let mut request = client
            .post(&endpoint)
            .header("Content-Type", "application/jsonl")
            .body(workflow.to_string());

        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }

        match request.send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();

                // Truncate large responses
                let truncated = if body.len() > MAX_RESPONSE_BYTES {
                    format!("{}...\n(response truncated at {} bytes)", &body[..MAX_RESPONSE_BYTES], MAX_RESPONSE_BYTES)
                } else {
                    body
                };

                if status.is_success() {
                    Ok(ToolOutput::success(truncated))
                } else {
                    // Don't leak server details — extract message if possible
                    let msg = serde_json::from_str::<Value>(&truncated)
                        .ok()
                        .and_then(|v| v.get("message").or(v.get("error")).and_then(|m| m.as_str()).map(String::from))
                        .unwrap_or_else(|| format!("A2E server error (HTTP {status})"));
                    Ok(ToolOutput::error(msg))
                }
            }
            Err(e) => {
                // Don't leak server URL in error
                tracing::error!("A2E connection failed: {e}");
                Ok(ToolOutput::error("Failed to connect to A2E server. Check A2E_SERVER_URL.".to_string()))
            }
        }
    }
}
