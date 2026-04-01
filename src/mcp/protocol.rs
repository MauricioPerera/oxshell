use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_ID: AtomicU64 = AtomicU64::new(0);

pub fn next_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

// ─── JSON-RPC 2.0 ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: next_id(),
            method: method.to_string(),
            params,
        }
    }

    /// Notification (no id, no response expected)
    pub fn notification(method: &str, params: Option<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[allow(dead_code)]
    pub data: Option<serde_json::Value>,
}

// ─── MCP Initialize ────────────────────────────────────

pub fn initialize_request() -> JsonRpcRequest {
    JsonRpcRequest::new(
        "initialize",
        Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "oxshell",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
    )
}

pub fn initialized_notification() -> serde_json::Value {
    JsonRpcRequest::notification("notifications/initialized", None)
}

// ─── MCP Tools ──────────────────────────────────────────

pub fn tools_list_request() -> JsonRpcRequest {
    JsonRpcRequest::new("tools/list", None)
}

pub fn tools_call_request(name: &str, arguments: &serde_json::Value) -> JsonRpcRequest {
    JsonRpcRequest::new(
        "tools/call",
        Some(serde_json::json!({
            "name": name,
            "arguments": arguments
        })),
    )
}

/// Parsed MCP tool definition
#[derive(Debug, Clone)]
pub struct MCPToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Parse tools/list result into MCPToolInfo vec
pub fn parse_tools_list(result: &serde_json::Value) -> Vec<MCPToolInfo> {
    result
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| {
                    let name = t.get("name")?.as_str()?.to_string();
                    let description = t
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input_schema = t
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or(serde_json::json!({"type": "object", "properties": {}}));
                    Some(MCPToolInfo {
                        name,
                        description,
                        input_schema,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse tools/call result content into a string
pub fn parse_tool_call_result(result: &serde_json::Value) -> String {
    // MCP returns: {"content": [{"type": "text", "text": "..."}]}
    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
        content
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    item.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    // For non-text content, serialize as JSON
                    Some(serde_json::to_string_pretty(item).unwrap_or_default())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else if let Some(text) = result.as_str() {
        text.to_string()
    } else {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
}
