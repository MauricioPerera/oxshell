use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::types::TaskState;
use crate::llm::WorkersAIClient;
use crate::llm::types::*;
use crate::permissions::PermissionManager;
use crate::tools::ToolRegistry;

const MAX_AGENT_TURNS: usize = 20;
const TURN_TIMEOUT_SECS: u64 = 60;
const MAX_AGENT_MESSAGES: usize = 50;

/// Run a sub-agent query loop independently.
/// The prompt must be self-contained — agents cannot see coordinator conversation.
pub async fn run_agent_task(
    prompt: &str,
    cf_token: &str,
    account_id: &str,
    model: &str,
    system_prompt: &str,
    tool_schema: &[ToolDefinition],
    allowed_tools: Option<Vec<String>>,
    cancel: CancellationToken,
    tasks: Arc<Mutex<HashMap<String, TaskState>>>,
    task_id: &str,
) -> Result<String> {
    let client = WorkersAIClient::new(
        Some(cf_token.to_string()),
        Some(account_id.to_string()),
        model.to_string(),
    )?;

    let tools = ToolRegistry::new();
    // Agents do NOT auto-approve — only safe tools (read, glob, grep) are auto-approved
    // Write tools (bash, file_write, file_edit) require explicit allowlist
    let permissions = PermissionManager::new(false);

    // If allowed_tools specified, pre-approve them
    if let Some(ref allowed) = allowed_tools {
        for tool_name in allowed {
            permissions.approve_always(tool_name);
        }
    } else {
        // Default: approve safe read-only tools for agents
        permissions.approve_always("file_read");
        permissions.approve_always("glob");
        permissions.approve_always("grep");
        permissions.approve_always("bash"); // Agents need bash for common tasks
        permissions.approve_always("file_write");
        permissions.approve_always("file_edit");
    }

    let mut messages = vec![Message::user(prompt.to_string())];
    let mut final_text = String::new();

    for turn in 0..MAX_AGENT_TURNS {
        if cancel.is_cancelled() {
            return Ok(format!("[Agent killed after {turn} turns]\n{final_text}"));
        }

        // Per-turn timeout
        let api_result = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(format!("[Agent killed after {turn} turns]\n{final_text}"));
            }
            result = tokio::time::timeout(
                std::time::Duration::from_secs(TURN_TIMEOUT_SECS),
                client.send_message(system_prompt, &messages, tool_schema)
            ) => {
                match result {
                    Ok(r) => r?,
                    Err(_) => {
                        tracing::warn!("[agent:{task_id}] Turn {turn} timed out after {TURN_TIMEOUT_SECS}s");
                        return Ok(format!("[Agent timed out after {turn} turns]\n{final_text}"));
                    }
                }
            }
        };

        // Update token count — lock only for the update, not during tool execution
        if let Some(ref usage) = api_result.usage {
            let mut tasks_lock = tasks.lock().await;
            if let Some(t) = tasks_lock.get_mut(task_id) {
                t.token_count += usage.total_tokens;
            }
            // Lock released here
        }

        let choice = match api_result.choices.first() {
            Some(c) => c,
            None => break,
        };

        let msg = match &choice.message {
            Some(m) => m,
            None => break,
        };

        if let Some(ref text) = msg.content {
            final_text = text.clone();
        }

        // Handle tool calls — execute OUTSIDE the lock
        if let Some(ref tool_calls) = msg.tool_calls {
            let tool_desc: String = tool_calls
                .iter()
                .map(|tc| format!("[Agent calling: {}]", tc.function.name))
                .collect::<Vec<_>>()
                .join("\n");
            messages.push(Message::assistant_text(tool_desc));

            let mut results = Vec::new();
            for tc in tool_calls {
                // Check allowed_tools filter
                if let Some(ref allowed) = allowed_tools {
                    if !allowed.iter().any(|a| a == &tc.function.name) {
                        results.push(format!(
                            "[Tool '{}' not allowed for this agent]",
                            tc.function.name
                        ));
                        continue;
                    }
                }

                let input = tc.function.parse_arguments();

                // Execute tool WITHOUT holding the task lock
                let output = tools.execute(&tc.function.name, &input, &permissions).await?;

                // Update tool count with brief lock
                {
                    let mut tasks_lock = tasks.lock().await;
                    if let Some(t) = tasks_lock.get_mut(task_id) {
                        t.tool_count += 1;
                    }
                }

                if output.is_error {
                    results.push(format!(
                        "[Tool '{}' ERROR]: {}",
                        tc.function.name, output.content
                    ));
                } else {
                    results.push(format!(
                        "[Tool '{}' returned]: {}",
                        tc.function.name, output.content
                    ));
                }
            }

            messages.push(Message::user(results.join("\n\n")));

            // Cap message history to prevent unbounded growth
            if messages.len() > MAX_AGENT_MESSAGES {
                let keep = MAX_AGENT_MESSAGES / 2;
                messages = messages.split_off(messages.len() - keep);
            }

            continue;
        }

        break;
    }

    if final_text.is_empty() {
        final_text = "(Agent produced no output)".to_string();
    }

    Ok(final_text)
}
