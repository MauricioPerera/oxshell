mod chat;
pub mod input;
mod render;
mod status;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use tokio::sync::mpsc;

use crate::context::Context;
use crate::llm::types::*;
use crate::llm::WorkersAIClient;
use crate::permissions::PermissionManager;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

pub use chat::ChatMessage;
pub use input::InputState;
pub use status::StatusInfo;

const MAX_CHAT_LOG: usize = 5000;

pub struct App {
    pub client: WorkersAIClient,
    pub tools: ToolRegistry,
    pub permissions: PermissionManager,
    pub context: Context,
    pub skills: SkillRegistry,
    pub messages: Vec<Message>,
    pub chat_log: Vec<ChatMessage>,
    pub input: InputState,
    pub scroll_offset: u16,
    pub status: StatusInfo,
    pub running: bool,
    pub waiting_for_response: bool,
    pub pending_approval: Option<PendingApproval>,
    pub total_usage: Usage,
    /// Receives notifications when background tasks complete
    pub task_notification_rx: tokio::sync::mpsc::Receiver<crate::tasks::manager::TaskNotification>,
}

pub struct PendingApproval {
    pub tool_name: String,
    pub tool_call_id: String,
    pub input: serde_json::Value,
    /// Remaining tool calls to process after this one
    pub remaining: Vec<(String, String, serde_json::Value)>,
}

impl App {
    pub fn new(
        client: WorkersAIClient,
        tools: ToolRegistry,
        permissions: PermissionManager,
        context: Context,
        skills: SkillRegistry,
        task_notification_rx: tokio::sync::mpsc::Receiver<crate::tasks::manager::TaskNotification>,
    ) -> Result<Self> {
        Ok(Self {
            client,
            tools,
            permissions,
            context,
            skills,
            messages: Vec::new(),
            chat_log: Vec::new(),
            input: InputState::new(),
            scroll_offset: 0,
            status: StatusInfo::default(),
            running: true,
            waiting_for_response: false,
            pending_approval: None,
            total_usage: Usage::default(),
            task_notification_rx,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.chat_log.push(ChatMessage::system(format!(
            "oxshell v{} — model: {} — type your message or /help",
            env!("CARGO_PKG_VERSION"),
            self.client.model
        )));

        let result = self.event_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        // Post-session: extract memories + flush
        let mut extractor = crate::memory::extraction::MemoryExtractor::new(
            &self.context.memory,
            &self.context.session_id,
        );
        let extracted = extractor.extract_from_messages(&self.messages).unwrap_or(0);
        if extracted > 0 {
            tracing::info!("Session end: extracted {extracted} memories");
        }
        let _ = extractor.save_session_summary(&self.messages);
        self.context.flush();

        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        let (response_tx, mut response_rx) = mpsc::channel::<AppEvent>(64);

        while self.running {
            terminal.draw(|f| render::draw(f, self))?;

            if event::poll(std::time::Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    match (key.modifiers, key.code) {
                        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                            self.running = false;
                        }
                        (_, KeyCode::Enter) if !self.input.buffer.is_empty() => {
                            if self.pending_approval.is_some() {
                                self.handle_approval_input(&response_tx).await;
                            } else if !self.waiting_for_response {
                                self.submit_message(&response_tx).await;
                            }
                        }
                        // Approval shortcuts
                        (_, KeyCode::Char('y'))
                            if self.pending_approval.is_some() && self.input.buffer.is_empty() =>
                        {
                            self.input.buffer = "y".to_string();
                            self.input.cursor = 1;
                            self.handle_approval_input(&response_tx).await;
                        }
                        (_, KeyCode::Char('n'))
                            if self.pending_approval.is_some() && self.input.buffer.is_empty() =>
                        {
                            self.input.buffer = "n".to_string();
                            self.input.cursor = 1;
                            self.handle_approval_input(&response_tx).await;
                        }
                        // Text input — UTF-8 safe via InputState
                        (_, KeyCode::Char(c)) => {
                            self.input.insert_char(c);
                        }
                        (_, KeyCode::Backspace) => {
                            self.input.backspace();
                        }
                        (_, KeyCode::Delete) => {
                            self.input.delete();
                        }
                        (_, KeyCode::Left) => self.input.move_left(),
                        (_, KeyCode::Right) => self.input.move_right(),
                        (_, KeyCode::Home) => self.input.move_home(),
                        (_, KeyCode::End) => self.input.move_end(),
                        // History navigation
                        (KeyModifiers::CONTROL, KeyCode::Up) => self.input.history_prev(),
                        (KeyModifiers::CONTROL, KeyCode::Down) => self.input.history_next(),
                        // Scroll
                        (_, KeyCode::Up) => {
                            self.scroll_offset = self.scroll_offset.saturating_add(1);
                        }
                        (_, KeyCode::Down) => {
                            self.scroll_offset = self.scroll_offset.saturating_sub(1);
                        }
                        (_, KeyCode::PageUp) => {
                            self.scroll_offset = self.scroll_offset.saturating_add(10);
                        }
                        (_, KeyCode::PageDown) => {
                            self.scroll_offset = self.scroll_offset.saturating_sub(10);
                        }
                        (_, KeyCode::Esc) => {
                            if self.pending_approval.is_some() {
                                self.pending_approval = None;
                                self.chat_log
                                    .push(ChatMessage::system("Tool execution cancelled.".into()));
                            }
                            self.input.clear();
                        }
                        _ => {}
                    }
                }
            }

            // Poll task notifications (from background tasks/agents)
            while let Ok(notif) = self.task_notification_rx.try_recv() {
                self.chat_log.push(ChatMessage::system(format!(
                    "Task {} completed",
                    notif.task_id
                )));
                // Inject notification XML into conversation for model to read
                self.messages.push(Message::user(notif.xml));
                // If we're not already waiting for a response, trigger one
                if !self.waiting_for_response {
                    self.waiting_for_response = true;
                    self.status.state = "Processing task result...".into();
                    self.send_to_api(&response_tx).await;
                }
            }

            // Process async events
            while let Ok(evt) = response_rx.try_recv() {
                match evt {
                    AppEvent::StreamDelta(text) => {
                        if let Some(last) = self.chat_log.last_mut() {
                            if last.role == "assistant" && last.streaming {
                                last.content.push_str(&text);
                                continue;
                            }
                        }
                        self.chat_log.push(ChatMessage::assistant_streaming(text));
                    }
                    AppEvent::StreamDone(response) => {
                        if let Some(last) = self.chat_log.last_mut() {
                            last.streaming = false;
                        }
                        if let Some(ref usage) = response.usage {
                            self.total_usage.accumulate(usage);
                            self.status.update_usage(&self.total_usage);
                        }

                        if let Some(choice) = response.choices.first().cloned() {
                            if let Some(ref msg) = choice.message {
                                if let Some(ref tool_calls) = msg.tool_calls {
                                    // Add assistant message
                                    self.messages.push(Message::assistant(
                                        msg.content.clone(),
                                        Some(tool_calls.clone()),
                                    ));

                                    // Process ALL tool calls, not just the first
                                    self.process_tool_calls(tool_calls.clone(), &response_tx).await;
                                } else {
                                    self.messages
                                        .push(Message::assistant(msg.content.clone(), None));
                                    self.waiting_for_response = false;
                                    self.status.state = "Ready".into();
                                }
                            }
                        }
                    }
                    AppEvent::ToolResult { id, output } => {
                        let content = output.content.clone();
                        let _is_error = output.is_error;
                        self.messages.push(Message::tool_result(id, content.clone()));
                        self.chat_log.push(ChatMessage::tool_result(content));
                        self.continue_conversation(&response_tx).await;
                    }
                    AppEvent::Error(e) => {
                        self.chat_log
                            .push(ChatMessage::error(format!("Error: {e}")));
                        self.waiting_for_response = false;
                        self.status.state = "Error".into();
                    }
                }
            }

            // Bound chat_log to prevent OOM in long sessions
            if self.chat_log.len() > MAX_CHAT_LOG {
                let drain = self.chat_log.len() - MAX_CHAT_LOG;
                self.chat_log.drain(..drain);
            }
        }

        Ok(())
    }

    /// Process all tool calls from a response, handling approvals sequentially
    async fn process_tool_calls(
        &mut self,
        tool_calls: Vec<ToolCall>,
        tx: &mpsc::Sender<AppEvent>,
    ) {
        let mut iter = tool_calls.into_iter();
        let first = match iter.next() {
            Some(tc) => tc,
            None => return,
        };

        let remaining: Vec<(String, String, serde_json::Value)> = iter
            .map(|tc| {
                let input = tc.function.parse_arguments();
                (tc.id, tc.function.name, input)
            })
            .collect();

        let input = first.function.parse_arguments();

        self.handle_tool_use(first.id, first.function.name, input, remaining, tx)
            .await;
    }

    async fn handle_tool_use(
        &mut self,
        id: String,
        name: String,
        input: serde_json::Value,
        remaining: Vec<(String, String, serde_json::Value)>,
        tx: &mpsc::Sender<AppEvent>,
    ) {
        let perm = self.tools.get_permission(&name);
        if self.permissions.needs_approval(&name, perm) {
            let desc = format_tool_description(&name, &input);
            self.chat_log.push(ChatMessage::system(format!(
                "Tool: {name}\n{desc}\n\nAllow? [y]es / [n]o / [a]lways"
            )));
            self.pending_approval = Some(PendingApproval {
                tool_name: name,
                tool_call_id: id,
                input,
                remaining,
            });
        } else {
            self.execute_tool(id, name, input, tx).await;
        }
    }

    async fn handle_approval_input(&mut self, tx: &mpsc::Sender<AppEvent>) {
        let response = self.input.submit().to_lowercase();

        if let Some(approval) = self.pending_approval.take() {
            match response.as_str() {
                "y" | "yes" => {
                    self.permissions.approve_session(&approval.tool_name);
                    self.execute_tool(
                        approval.tool_call_id,
                        approval.tool_name,
                        approval.input,
                        tx,
                    )
                    .await;
                }
                "a" | "always" => {
                    self.permissions.approve_always(&approval.tool_name);
                    self.chat_log.push(ChatMessage::system(format!(
                        "Tool '{}' permanently approved.",
                        approval.tool_name
                    )));
                    self.execute_tool(
                        approval.tool_call_id,
                        approval.tool_name,
                        approval.input,
                        tx,
                    )
                    .await;
                }
                _ => {
                    let result = format!("User denied permission for tool: {}", approval.tool_name);
                    self.messages
                        .push(Message::tool_result(approval.tool_call_id, result.clone()));
                    self.chat_log.push(ChatMessage::system(result));
                    self.continue_conversation(tx).await;
                }
            }
        }
    }

    async fn execute_tool(
        &mut self,
        id: String,
        name: String,
        input: serde_json::Value,
        tx: &mpsc::Sender<AppEvent>,
    ) {
        self.status.state = format!("Running {name}...");
        self.chat_log.push(ChatMessage::tool_running(name.clone()));

        let tx = tx.clone();
        // Use the real permission manager — tool was already approved before reaching here
        let tool_result = self.tools.execute(&name, &input, &self.permissions).await;
        match tool_result {
            Ok(output) => {
                let _ = tx
                    .send(AppEvent::ToolResult { id, output })
                    .await;
            }
            Err(e) => {
                let _ = tx.send(AppEvent::Error(e.to_string())).await;
            }
        }
    }

    async fn submit_message(&mut self, tx: &mpsc::Sender<AppEvent>) {
        let text = self.input.submit();
        self.scroll_offset = 0;

        if text.is_empty() {
            return;
        }

        if text.starts_with('/') {
            // Check if it's a skill invocation (e.g., /commit, /review)
            let (cmd, args) = text.split_once(' ').unwrap_or((&text, ""));
            let skill_name = &cmd[1..]; // strip leading /

            if let Some(skill) = self.skills.get(skill_name).cloned() {
                // Execute skill inline — inject rendered prompt as user message
                let skill = skill;
                let rendered = skill.render(args);
                self.chat_log.push(ChatMessage::system(format!(
                    "Running skill: {} {}",
                    skill.name,
                    if args.is_empty() { "" } else { args }
                )));
                self.messages.push(Message::user(rendered));
                self.waiting_for_response = true;
                self.status.state = format!("Running /{skill_name}...");
                self.send_to_api(tx).await;
                return;
            }

            self.handle_command(&text);
            return;
        }

        self.chat_log.push(ChatMessage::user(text.clone()));
        self.messages.push(Message::user(text));
        self.waiting_for_response = true;
        self.status.state = "Thinking...".into();

        self.send_to_api(tx).await;
    }

    async fn send_to_api(&self, tx: &mpsc::Sender<AppEvent>) {
        let mut system = self.context.build_system_prompt();

        // Add skills section
        let skills_section = self.skills.prompt_section();
        if !skills_section.is_empty() {
            system.push_str("\n\n");
            system.push_str(&skills_section);
        }

        // Add relevant memories
        if let Some(last_user) = self.messages.iter().rev().find(|m| m.role == Role::User) {
            let query = last_user.content.as_deref().unwrap_or("");
            let relevant = self.context.build_relevant_memories(query);
            if !relevant.is_empty() {
                system.push_str("\n\n");
                system.push_str(&relevant);
            }
        }
        let messages = self.messages.clone();
        let tools = self.tools.schema();
        let tx = tx.clone();

        let (cf_token, account_id, model) = self.client.credentials();

        tokio::spawn(async move {
            let client = match WorkersAIClient::new(Some(cf_token), Some(account_id), model) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(AppEvent::Error(e.to_string())).await;
                    return;
                }
            };

            match client
                .send_message_streaming(&system, &messages, &tools)
                .await
            {
                Ok(mut rx) => {
                    while let Some(event) = rx.recv().await {
                        match event {
                            StreamEvent::TextDelta(text) => {
                                let _ = tx.send(AppEvent::StreamDelta(text)).await;
                            }
                            StreamEvent::ToolCallStart { name, .. } => {
                                let _ = tx
                                    .send(AppEvent::StreamDelta(format!("\n[tool: {name}]\n")))
                                    .await;
                            }
                            StreamEvent::Done(response) => {
                                let _ = tx.send(AppEvent::StreamDone(response)).await;
                            }
                            StreamEvent::Error(e) => {
                                let _ = tx.send(AppEvent::Error(e)).await;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Error(e.to_string())).await;
                }
            }
        });
    }

    async fn continue_conversation(&mut self, tx: &mpsc::Sender<AppEvent>) {
        self.status.state = "Thinking...".into();
        self.send_to_api(tx).await;
    }

    fn handle_command(&mut self, cmd: &str) {
        match cmd.trim() {
            "/help" => {
                self.chat_log.push(ChatMessage::system(
                    "Commands:\n  /help      - Show this help\n  /clear     - Clear chat\n  /cost      - Show token usage\n  /memory    - Show memory stats\n  /skills    - List available skills\n  /model     - Show current model\n  /exit      - Quit\n  /<skill>   - Run a skill (e.g. /commit, /review)\n\nKeys:\n  Ctrl+Up/Down  - Input history\n  Up/Down       - Scroll chat\n  Esc           - Cancel / clear input"
                        .to_string(),
                ));
            }
            "/clear" => {
                self.chat_log.clear();
                self.messages.clear();
                self.chat_log
                    .push(ChatMessage::system("Chat cleared.".into()));
            }
            "/cost" => {
                let u = &self.total_usage;
                self.chat_log.push(ChatMessage::system(format!(
                    "Tokens: {} in / {} out / {} total\nEst. cost: {}",
                    u.prompt_tokens,
                    u.completion_tokens,
                    u.total_tokens,
                    u.format_cost()
                )));
            }
            "/model" => {
                self.chat_log.push(ChatMessage::system(format!(
                    "Model: {}",
                    self.client.model
                )));
            }
            "/memory" | "/mem" => {
                let count = self.context.memory.count();
                let headers = self.context.memory.scan_headers().unwrap_or_default();
                let by_type: std::collections::HashMap<&str, usize> = {
                    let mut map = std::collections::HashMap::new();
                    for h in &headers {
                        *map.entry(h.memory_type.as_str()).or_insert(0) += 1;
                    }
                    map
                };
                let type_info: String = by_type
                    .iter()
                    .map(|(t, c)| format!("  {t}: {c}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                let recent: String = headers
                    .iter()
                    .take(5)
                    .map(|h| format!("  [{}] {}", h.memory_type.as_str(), h.name))
                    .collect::<Vec<_>>()
                    .join("\n");

                self.chat_log.push(ChatMessage::system(format!(
                    "Memory: {count} entries\n\nBy type:\n{type_info}\n\nRecent:\n{recent}"
                )));
            }
            "/skills" => {
                let skills = self.skills.user_invocable();
                if skills.is_empty() {
                    self.chat_log.push(ChatMessage::system(
                        "No skills available. Create .oxshell/skills/<name>/SKILL.md to add one.".into(),
                    ));
                } else {
                    let list: String = skills
                        .iter()
                        .map(|s| {
                            let src = match s.source {
                                crate::skills::types::SkillSource::Bundled => "built-in",
                                crate::skills::types::SkillSource::Filesystem => "custom",
                            };
                            format!("  /{} - {} [{}]", s.name, s.description, src)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.chat_log.push(ChatMessage::system(format!(
                        "Available skills ({}):\n{}\n\nUsage: /<skill> [args]",
                        skills.len(),
                        list
                    )));
                }
            }
            "/exit" | "/quit" | "/q" => {
                self.running = false;
            }
            _ => {
                self.chat_log
                    .push(ChatMessage::system(format!("Unknown command: {cmd}")));
            }
        }
    }
}

enum AppEvent {
    StreamDelta(String),
    StreamDone(ChatCompletionResponse),
    ToolResult {
        id: String,
        output: crate::tools::ToolOutput,
    },
    Error(String),
}

fn format_tool_description(name: &str, input: &serde_json::Value) -> String {
    match name {
        "bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!("$ {cmd}")
        }
        "file_write" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Write to: {path}")
        }
        "file_edit" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Edit: {path}")
        }
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}
