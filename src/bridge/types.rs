use serde::{Deserialize, Serialize};

/// Request to send a prompt to oxshell
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PromptRequest {
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Response from a prompt execution
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct PromptResponse {
    pub session_id: String,
    pub response: String,
    pub tool_calls: Vec<ToolCallInfo>,
    pub usage: UsageInfo,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ToolCallInfo {
    pub name: String,
    pub result: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Server status
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct StatusResponse {
    pub version: String,
    pub model: String,
    pub session_count: usize,
    pub memory_count: usize,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub uptime_secs: u64,
}

/// WebSocket message (client → server)
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum WsClientMessage {
    Prompt { text: String },
    Cancel,
    Approve { tool_name: String },
    Deny { tool_name: String },
}

/// WebSocket message (server → client)
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum WsServerMessage {
    /// Streaming text delta
    TextDelta { text: String },
    /// Tool is about to execute (needs approval if not auto-approved)
    ToolUse { name: String, input: String },
    /// Tool completed
    ToolResult { name: String, result: String, is_error: bool },
    /// Response complete
    Done { session_id: String, usage: UsageInfo },
    /// Error occurred
    Error { message: String },
    /// Server status
    Status { model: String, session_id: String },
}
