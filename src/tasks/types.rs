use std::time::{Duration, SystemTime};

/// Task ID: prefix char + 8 random alphanumeric
pub type TaskId = String;

/// Generate a new task ID with the given prefix
pub fn new_task_id(prefix: char) -> TaskId {
    use uuid::Uuid;
    let uuid = Uuid::new_v4().to_string().replace('-', "");
    format!("{}{}", prefix, &uuid[..8])
}

/// Task types supported by oxshell
#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    /// Background shell command (prefix: b)
    Bash,
    /// Sub-agent with own query loop (prefix: a)
    Agent,
}

impl TaskType {
    pub fn prefix(&self) -> char {
        match self {
            Self::Bash => 'b',
            Self::Agent => 'a',
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Agent => "agent",
        }
    }
}

/// Task lifecycle states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
}

impl TaskStatus {
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Killed)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Killed => "killed",
        }
    }
}

/// State for a running or completed task
#[derive(Debug, Clone)]
pub struct TaskState {
    pub id: TaskId,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub description: String,
    /// The original prompt/command that created this task
    pub input: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    /// Accumulated output text
    pub output: String,
    /// Whether the coordinator has been notified
    pub notified: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// For agent tasks: model used
    pub model: Option<String>,
    /// For agent tasks: number of tool calls made
    pub tool_count: u32,
    /// For agent tasks: token usage
    pub token_count: u32,
}

impl TaskState {
    pub fn new(task_type: TaskType, description: &str) -> Self {
        let id = new_task_id(task_type.prefix());
        Self {
            id,
            task_type,
            status: TaskStatus::Pending,
            description: description.to_string(),
            input: String::new(),
            start_time: SystemTime::now(),
            end_time: None,
            output: String::new(),
            notified: false,
            error: None,
            model: None,
            tool_count: 0,
            token_count: 0,
        }
    }

    pub fn complete(&mut self, output: String) {
        self.status = TaskStatus::Completed;
        self.end_time = Some(SystemTime::now());
        self.output = output;
    }

    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.end_time = Some(SystemTime::now());
        self.error = Some(error);
    }

    pub fn kill(&mut self) {
        self.status = TaskStatus::Killed;
        self.end_time = Some(SystemTime::now());
    }

    pub fn duration(&self) -> Duration {
        let end = self.end_time.unwrap_or_else(SystemTime::now);
        end.duration_since(self.start_time).unwrap_or_default()
    }

    pub fn duration_ms(&self) -> u128 {
        self.duration().as_millis()
    }

    /// Format as XML notification (like Claude Code's task-notification)
    pub fn to_notification(&self) -> String {
        let summary = if let Some(ref err) = self.error {
            format!("Error: {}", xml_escape(err))
        } else if self.output.len() > 500 {
            format!("{}...", xml_escape(&self.output[..500]))
        } else {
            xml_escape(&self.output)
        };

        format!(
            "<task-notification>\n\
             <task-id>{}</task-id>\n\
             <status>{}</status>\n\
             <type>{}</type>\n\
             <description>{}</description>\n\
             <duration-ms>{}</duration-ms>\n\
             <tool-calls>{}</tool-calls>\n\
             <tokens>{}</tokens>\n\
             <summary>{}</summary>\n\
             </task-notification>",
            self.id,
            self.status.as_str(),
            self.task_type.label(),
            xml_escape(&self.description),
            self.duration_ms(),
            self.tool_count,
            self.token_count,
            summary
        )
    }
}

/// Escape XML special characters
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
