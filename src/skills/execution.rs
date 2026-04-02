use anyhow::{Result, bail};

use super::types::*;
use crate::llm::WorkersAIClient;
use crate::llm::types::Message;
use crate::permissions::PermissionManager;
use crate::tools::ToolRegistry;

/// Execute a skill, returning the result text.
///
/// Two modes:
/// - **Inline**: Renders the prompt and returns it as a user message to inject
/// - **Fork**: Runs a full conversation loop in isolation and returns the final text
pub async fn execute_skill(
    skill: &Skill,
    args: &str,
    client: &WorkersAIClient,
    tools: &ToolRegistry,
    permissions: &PermissionManager,
    system_prompt: &str,
) -> Result<SkillResult> {
    let rendered = skill.render(args);

    match skill.context {
        SkillContext::Inline => Ok(SkillResult::Inline(rendered)),
        SkillContext::Fork => {
            execute_forked(skill, &rendered, client, tools, permissions, system_prompt).await
        }
    }
}

/// Result of skill execution
pub enum SkillResult {
    /// Prompt to inject as user message (inline mode)
    Inline(String),
    /// Completed result text from forked execution
    Forked(String),
}

/// Run a skill in an isolated sub-agent conversation loop.
/// Has its own message history, limited to 10 turns.
async fn execute_forked(
    skill: &Skill,
    rendered_prompt: &str,
    client: &WorkersAIClient,
    tools: &ToolRegistry,
    permissions: &PermissionManager,
    system_prompt: &str,
) -> Result<SkillResult> {
    const MAX_TURNS: usize = 10;

    let mut messages = vec![Message::user(rendered_prompt.to_string())];
    let tool_schema = tools.schema();
    let mut final_text = String::new();

    for _turn in 0..MAX_TURNS {
        let response = client
            .send_message(system_prompt, &messages, &tool_schema)
            .await?;

        let choice = match response.choices.first() {
            Some(c) => c,
            None => break,
        };

        let msg = match &choice.message {
            Some(m) => m,
            None => break,
        };

        // Collect text output
        if let Some(ref text) = msg.content {
            final_text = text.clone();
        }

        // Handle tool calls
        if let Some(ref tool_calls) = msg.tool_calls {
            let tool_desc: String = tool_calls
                .iter()
                .map(|tc| {
                    format!(
                        "[Skill '{}' calling: {} ({})]",
                        skill.name, tc.function.name, tc.function.arguments
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            messages.push(Message::assistant_text(tool_desc));

            let mut results = Vec::new();
            for tc in tool_calls {
                // Check if tool is in allowed list (if specified)
                if !skill.allowed_tools.is_empty() {
                    let allowed = skill.allowed_tools.iter().any(|t| {
                        t.eq_ignore_ascii_case(&tc.function.name)
                            || t == "shell" && tc.function.name == "bash"
                    });
                    if !allowed {
                        results.push(format!(
                            "[Tool '{}' not in allowed-tools for skill '{}']",
                            tc.function.name, skill.name
                        ));
                        continue;
                    }
                }

                let input: serde_json::Value =
                    tc.function.parse_arguments();
                let output = tools.execute(&tc.function.name, &input, permissions).await?;
                results.push(format!(
                    "[Tool '{}' returned]:\n{}",
                    tc.function.name, output.content
                ));
            }

            messages.push(Message::user(results.join("\n\n")));
            continue;
        }

        // No tool calls — skill complete
        break;
    }

    if final_text.is_empty() {
        bail!("Skill '{}' produced no output after {MAX_TURNS} turns", skill.name);
    }

    Ok(SkillResult::Forked(final_text))
}
