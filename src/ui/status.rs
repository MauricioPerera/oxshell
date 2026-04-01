use crate::llm::types::Usage;

/// Status bar information
#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub state: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

impl Default for StatusInfo {
    fn default() -> Self {
        Self {
            state: "Ready".into(),
            tokens_in: 0,
            tokens_out: 0,
        }
    }
}

impl StatusInfo {
    pub fn update_usage(&mut self, usage: &Usage) {
        self.tokens_in = usage.prompt_tokens;
        self.tokens_out = usage.completion_tokens;
        self.state = "Ready".into();
    }
}
