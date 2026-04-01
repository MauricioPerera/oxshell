use serde::{Deserialize, Serialize};

/// Events that can trigger hooks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Before a tool is executed
    PreToolUse,
    /// After a tool completes successfully
    PostToolUse,
    /// After a tool fails
    PostToolUseFailure,
    /// Before user prompt is sent to LLM
    UserPromptSubmit,
    /// When a session starts
    SessionStart,
    /// When a session ends
    SessionEnd,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "pre_tool_use",
            Self::PostToolUse => "post_tool_use",
            Self::PostToolUseFailure => "post_tool_use_failure",
            Self::UserPromptSubmit => "user_prompt_submit",
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
        }
    }
}

/// What a hook can do
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Allow the operation to proceed (default)
    Allow,
    /// Block the operation with a reason
    Block(String),
    /// Modify the input/output
    Modify(String),
}

/// A configured hook — shell command or inline matcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Which event triggers this hook
    pub event: HookEvent,
    /// Optional tool name filter (only trigger for this tool)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    /// Shell command to execute (receives context via env vars)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Inline script (evaluated as shell)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// Timeout in milliseconds (default 10000)
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    10_000
}

/// Settings file hook configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksSettings {
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
}
