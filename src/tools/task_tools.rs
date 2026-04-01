use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;
use crate::tasks::TaskManager;

// ─── Spawn Agent Tool ───────────────────────────────────

pub struct SpawnAgentTool {
    pub task_manager: Arc<Mutex<TaskManager>>,
    pub cf_token: String,
    pub account_id: String,
    pub model: String,
    /// Built dynamically on each call via context
    pub system_prompt_fn: Arc<dyn Fn() -> String + Send + Sync>,
    pub tool_schema: Vec<crate::llm::types::ToolDefinition>,
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str { "spawn_agent" }

    fn description(&self) -> &str {
        "Spawn a background sub-agent worker. Returns task ID immediately. \
         The worker runs independently — prompt must be SELF-CONTAINED."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Self-contained prompt with ALL context the worker needs."
                },
                "description": {
                    "type": "string",
                    "description": "Short label for task list"
                },
                "allowed_tools": {
                    "type": "string",
                    "description": "Comma-separated list of tools this agent can use. Default: all."
                }
            },
            "required": ["prompt", "description"]
        })
    }

    fn permission(&self) -> ToolPermission { ToolPermission::RequiresApproval }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let prompt = input.get("prompt").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'prompt'"))?;
        let description = input.get("description").and_then(|v| v.as_str())
            .unwrap_or("agent worker");

        let allowed_tools: Option<Vec<String>> = input
            .get("allowed_tools")
            .and_then(|v| v.as_str())
            .map(|s| s.split(',').map(|t| t.trim().to_string()).collect());

        // Build system prompt fresh (includes coordinator context)
        let system_prompt = (self.system_prompt_fn)();

        let manager = self.task_manager.lock().await;
        let task_id = manager.spawn_agent(
            prompt,
            description,
            self.cf_token.clone(),
            self.account_id.clone(),
            self.model.clone(),
            system_prompt,
            self.tool_schema.clone(),
            allowed_tools,
        ).await?;

        Ok(ToolOutput::success(format!(
            "Agent spawned: {task_id}\nYou will receive a <task-notification> when it completes."
        )))
    }
}

// ─── Spawn Bash Tool ────────────────────────────────────

pub struct SpawnBashTool {
    pub task_manager: Arc<Mutex<TaskManager>>,
}

#[async_trait]
impl Tool for SpawnBashTool {
    fn name(&self) -> &str { "spawn_bash" }
    fn description(&self) -> &str { "Run a shell command in the background. Returns task ID." }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "description": { "type": "string", "description": "Short label" }
            },
            "required": ["command"]
        })
    }

    fn permission(&self) -> ToolPermission { ToolPermission::RequiresApproval }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let command = input.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command'"))?;
        let description = input.get("description").and_then(|v| v.as_str())
            .unwrap_or(command);

        let manager = self.task_manager.lock().await;
        let task_id = manager.spawn_bash(command, description).await?;

        Ok(ToolOutput::success(format!("Background task: {task_id}")))
    }
}

// ─── Task List Tool ─────────────────────────────────────

pub struct TaskListTool {
    pub task_manager: Arc<Mutex<TaskManager>>,
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str { "task_list" }
    fn description(&self) -> &str { "List all tasks with status." }
    fn input_schema(&self) -> Value { json!({ "type": "object", "properties": {} }) }
    fn permission(&self) -> ToolPermission { ToolPermission::AutoApprove }

    async fn execute(&self, _input: &Value) -> Result<ToolOutput> {
        let manager = self.task_manager.lock().await;
        let tasks = manager.list().await;

        if tasks.is_empty() {
            return Ok(ToolOutput::success("No tasks.".to_string()));
        }

        let lines: Vec<String> = tasks.iter().map(|t| {
            format!(
                "{} [{}] {} — {} ({}ms, {} tools, {} tokens)",
                t.id, t.status.as_str(), t.task_type.label(),
                t.description, t.duration_ms(), t.tool_count, t.token_count
            )
        }).collect();

        Ok(ToolOutput::success(lines.join("\n")))
    }
}

// ─── Task Stop Tool ─────────────────────────────────────

pub struct TaskStopTool {
    pub task_manager: Arc<Mutex<TaskManager>>,
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str { "task_stop" }
    fn description(&self) -> &str { "Stop a running task by ID." }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "Task ID to stop" }
            },
            "required": ["task_id"]
        })
    }

    fn permission(&self) -> ToolPermission { ToolPermission::RequiresApproval }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let task_id = input.get("task_id").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'task_id'"))?;

        let manager = self.task_manager.lock().await;
        if manager.kill(task_id).await? {
            Ok(ToolOutput::success(format!("Task {task_id} stopped.")))
        } else {
            Ok(ToolOutput::error(format!("Task {task_id} not found.")))
        }
    }
}
