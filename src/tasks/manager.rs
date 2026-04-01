use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use super::agent::run_agent_task;
use super::types::*;

/// A notification that a task completed/failed/was killed
pub struct TaskNotification {
    pub task_id: String,
    pub xml: String,
}

/// Manages all running and completed tasks.
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, TaskState>>>,
    cancellation_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    notification_tx: mpsc::Sender<TaskNotification>,
}

impl TaskManager {
    /// Create a new TaskManager and its notification receiver.
    /// The receiver MUST be polled by the caller (TUI event loop or oneshot loop).
    pub fn new() -> (Self, mpsc::Receiver<TaskNotification>) {
        let (tx, rx) = mpsc::channel(512); // Large buffer to avoid blocking
        let manager = Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            cancellation_tokens: Arc::new(Mutex::new(HashMap::new())),
            notification_tx: tx,
        };
        (manager, rx)
    }

    // ─── Spawn Tasks ────────────────────────────────────

    /// Spawn a background bash command
    pub async fn spawn_bash(&self, command: &str, description: &str) -> Result<TaskId> {
        let mut task = TaskState::new(TaskType::Bash, description);
        task.input = command.to_string();
        task.status = TaskStatus::Running;
        let task_id = task.id.clone();
        let token = CancellationToken::new();

        {
            let mut tasks = self.tasks.lock().await;
            tasks.insert(task_id.clone(), task);
        }
        {
            let mut tokens = self.cancellation_tokens.lock().await;
            tokens.insert(task_id.clone(), token.clone());
        }

        let tasks = self.tasks.clone();
        let tx = self.notification_tx.clone();
        let cmd = command.to_string();
        let id = task_id.clone();

        tokio::spawn(async move {
            let (shell, flag) = if cfg!(target_os = "windows") {
                ("cmd", "/C")
            } else {
                ("bash", "-c")
            };

            let result = tokio::select! {
                _ = token.cancelled() => {
                    let mut tasks = tasks.lock().await;
                    if let Some(t) = tasks.get_mut(&id) {
                        t.kill();
                        let xml = t.to_notification();
                        t.notified = true;
                        if let Err(e) = tx.send(TaskNotification { task_id: id.clone(), xml }).await {
                            tracing::warn!("Failed to send task notification: {e}");
                        }
                    }
                    return;
                }
                result = Command::new(shell).arg(flag).arg(&cmd).output() => result
            };

            let mut tasks = tasks.lock().await;
            if let Some(t) = tasks.get_mut(&id) {
                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let mut text = stdout;
                        if !stderr.is_empty() {
                            text.push_str(&format!("\nstderr: {stderr}"));
                        }
                        let code = output.status.code().unwrap_or(-1);
                        if code != 0 {
                            text.push_str(&format!("\n(exit code: {code})"));
                        }
                        t.complete(text);
                    }
                    Err(e) => t.fail(format!("Command failed: {e}")),
                }
                let xml = t.to_notification();
                t.notified = true;
                if let Err(e) = tx.send(TaskNotification { task_id: id, xml }).await {
                    tracing::warn!("Failed to send task notification: {e}");
                }
            }
        });

        Ok(task_id)
    }

    /// Spawn a sub-agent with its own query loop
    pub async fn spawn_agent(
        &self,
        prompt: &str,
        description: &str,
        cf_token: String,
        account_id: String,
        model: String,
        system_prompt: String,
        tool_schema: Vec<crate::llm::types::ToolDefinition>,
        allowed_tools: Option<Vec<String>>,
    ) -> Result<TaskId> {
        let mut task = TaskState::new(TaskType::Agent, description);
        task.input = prompt.to_string();
        task.status = TaskStatus::Running;
        task.model = Some(model.clone());
        let task_id = task.id.clone();
        let token = CancellationToken::new();

        {
            let mut tasks = self.tasks.lock().await;
            tasks.insert(task_id.clone(), task);
        }
        {
            let mut tokens = self.cancellation_tokens.lock().await;
            tokens.insert(task_id.clone(), token.clone());
        }

        let tasks = self.tasks.clone();
        let tx = self.notification_tx.clone();
        let prompt = prompt.to_string();
        let id = task_id.clone();

        tokio::spawn(async move {
            let result = run_agent_task(
                &prompt,
                &cf_token,
                &account_id,
                &model,
                &system_prompt,
                &tool_schema,
                allowed_tools,
                token,
                tasks.clone(),
                &id,
            )
            .await;

            let mut tasks = tasks.lock().await;
            if let Some(t) = tasks.get_mut(&id) {
                match result {
                    Ok(output) => t.complete(output),
                    Err(e) => t.fail(e.to_string()),
                }
                let xml = t.to_notification();
                t.notified = true;
                if let Err(e) = tx.send(TaskNotification { task_id: id.clone(), xml }).await {
                    tracing::warn!("Failed to send task notification: {e}");
                }
            }
        });

        Ok(task_id)
    }

    // ─── Control ────────────────────────────────────────

    pub async fn kill(&self, task_id: &str) -> Result<bool> {
        let tokens = self.cancellation_tokens.lock().await;
        if let Some(token) = tokens.get(task_id) {
            token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Shutdown all running tasks
    pub async fn shutdown(&self) {
        let tokens = self.cancellation_tokens.lock().await;
        for token in tokens.values() {
            token.cancel();
        }
    }

    // ─── Query ──────────────────────────────────────────

    pub async fn get(&self, task_id: &str) -> Option<TaskState> {
        let tasks = self.tasks.lock().await;
        tasks.get(task_id).cloned()
    }

    pub async fn list(&self) -> Vec<TaskState> {
        let tasks = self.tasks.lock().await;
        tasks.values().cloned().collect()
    }

}
