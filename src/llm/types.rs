use serde::{Deserialize, Serialize};

// ─── Messages (OpenAI-compatible) ───────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: String) -> Self {
        Self {
            role: Role::System,
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: String) -> Self {
        Self {
            role: Role::User,
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: Option<String>, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    pub fn assistant_text(content: String) -> Self {
        Self::assistant(Some(content), None)
    }

    pub fn tool_result(tool_call_id: String, content: String) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        }
    }

    /// Extract text content
    pub fn text(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

// ─── Tool Calls (OpenAI format) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

impl FunctionCall {
    /// Parse arguments JSON, handling double-escaped strings from some models
    /// (e.g., Granite returns `"\"{ JSON }\"` instead of `"{ JSON }"`).
    pub fn parse_arguments(&self) -> serde_json::Value {
        // Try direct parse first
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&self.arguments) {
            // If it parsed as a string, try parsing the inner string as JSON
            if let serde_json::Value::String(inner) = &v {
                if let Ok(inner_v) = serde_json::from_str::<serde_json::Value>(inner) {
                    if inner_v.is_object() {
                        return inner_v;
                    }
                }
            }
            // If it's already an object, return it
            if v.is_object() {
                return v;
            }
            return v;
        }
        // Fallback: try stripping outer quotes and unescaping
        let trimmed = self.arguments.trim().trim_matches('"');
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return v;
        }
        serde_json::Value::Object(serde_json::Map::new())
    }
}

// ─── API Request (OpenAI-compatible) ────────────────────

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

// ─── API Response (OpenAI-compatible) ───────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    #[serde(default)]
    pub id: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    #[serde(default)]
    pub message: Option<ChatMessage>,
    #[serde(default)]
    pub delta: Option<DeltaMessage>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessage {
    pub role: Option<Role>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaMessage {
    pub role: Option<Role>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaToolCall {
    pub index: Option<u32>,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<DeltaFunction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

// ─── Usage ──────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

impl Usage {
    pub fn accumulate(&mut self, other: &Usage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
    }

    /// Workers AI pricing (as of 2026):
    /// - Regular neurons: $0.011 / 1K neurons
    /// - Most small models on free tier: 10K neurons/day free
    /// Tokens ≈ neurons for text models (rough approximation)
    pub fn estimated_cost(&self) -> f64 {
        (self.total_tokens as f64) * 0.011 / 1_000.0
    }

    pub fn format_cost(&self) -> String {
        let cost = self.estimated_cost();
        if cost == 0.0 {
            "$0".to_string()
        } else if cost < 0.01 {
            format!("${:.4}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }
}

// ─── Tool Definitions (OpenAI function calling) ─────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── Streaming Events ───────────────────────────────────

#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text delta from assistant
    TextDelta(String),
    /// Tool call started (id, name)
    ToolCallStart { index: u32, id: String, name: String },
    /// Tool call arguments delta
    ToolCallArgsDelta { index: u32, args: String },
    /// Stream finished — full aggregated response
    Done(ChatCompletionResponse),
    /// Error
    Error(String),
}
