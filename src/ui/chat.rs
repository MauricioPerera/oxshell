/// A message displayed in the chat log
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub streaming: bool,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".into(),
            content,
            streaming: false,
        }
    }

    pub fn assistant_streaming(content: String) -> Self {
        Self {
            role: "assistant".into(),
            content,
            streaming: true,
        }
    }

    pub fn system(content: String) -> Self {
        Self {
            role: "system".into(),
            content,
            streaming: false,
        }
    }

    pub fn error(content: String) -> Self {
        Self {
            role: "error".into(),
            content,
            streaming: false,
        }
    }

    pub fn tool_result(content: String) -> Self {
        Self {
            role: "tool".into(),
            content: if content.len() > 500 {
                format!("{}...\n(truncated)", &content[..500])
            } else {
                content
            },
            streaming: false,
        }
    }

    pub fn tool_running(name: String) -> Self {
        Self {
            role: "tool".into(),
            content: format!("Running {name}..."),
            streaming: false,
        }
    }
}
