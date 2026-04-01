use anyhow::Result;

use crate::llm::WorkersAIClient;
use crate::llm::types::Message;

const COMPACTION_THRESHOLD: f64 = 0.80;
const KEEP_RECENT_MESSAGES: usize = 6;

/// Result of a compaction operation
pub struct CompactionResult {
    pub compacted_messages: Vec<Message>,
    pub original_count: usize,
    pub compacted_count: usize,
}

/// Estimate token count from messages (chars/4 approximation)
pub fn estimate_tokens(messages: &[Message], system_prompt: &str) -> usize {
    let system_chars = system_prompt.len();
    let msg_chars: usize = messages
        .iter()
        .map(|m| {
            let content_len = m.content.as_ref().map_or(0, |c| c.len());
            let tool_len = m
                .tool_calls
                .as_ref()
                .map_or(0, |tcs| {
                    tcs.iter()
                        .map(|tc| tc.function.name.len() + tc.function.arguments.len())
                        .sum::<usize>()
                });
            content_len + tool_len + 20 // per-message overhead
        })
        .sum();
    (system_chars + msg_chars) / 4
}

/// Known context window limits for Workers AI models
pub fn model_context_limit(model: &str) -> usize {
    let m = model.to_lowercase();
    if m.contains("gpt-oss") {
        128_000
    } else if m.contains("nemotron") {
        256_000
    } else if m.contains("granite") {
        131_072
    } else if m.contains("llama-3.1") || m.contains("llama-4") {
        131_072
    } else if m.contains("llama-3.3") {
        24_000 // fp8-fast variant has 24K context
    } else if m.contains("llama-3") {
        8_192
    } else if m.contains("qwen2.5-coder-32b") || m.contains("qwen2.5-coder-14b") {
        32_768
    } else if m.contains("deepseek") {
        16_384
    } else if m.contains("qwen") {
        32_768
    } else if m.contains("mistral-small-3") {
        128_000
    } else if m.contains("mistral") || m.contains("hermes") {
        4_096
    } else {
        4_096 // conservative default
    }
}

/// Check if compaction is needed and perform it if so.
/// Returns None if conversation is within limits.
pub async fn maybe_compact(
    client: &WorkersAIClient,
    messages: &[Message],
    system_prompt: &str,
) -> Result<Option<CompactionResult>> {
    let estimated = estimate_tokens(messages, system_prompt);
    let limit = model_context_limit(&client.model);
    let threshold = (limit as f64 * COMPACTION_THRESHOLD) as usize;

    if estimated < threshold {
        return Ok(None);
    }

    if messages.len() <= KEEP_RECENT_MESSAGES + 1 {
        // Too few messages to compact meaningfully
        return Ok(None);
    }

    tracing::info!(
        "Context compaction triggered: ~{estimated} tokens estimated, limit {limit} ({}% full)",
        (estimated * 100) / limit
    );

    let original_count = messages.len();
    let split_point = messages.len().saturating_sub(KEEP_RECENT_MESSAGES);
    let old_messages = &messages[..split_point];
    let recent_messages = &messages[split_point..];

    // Format old messages into a summary request
    let conversation_text = format_for_summarization(old_messages);

    let summary_prompt = format!(
        "Summarize this conversation history concisely. Preserve:\n\
         - Key decisions and reasoning\n\
         - Tool results (file paths, command outputs)\n\
         - Code changes (file names, what was modified)\n\
         - Unresolved tasks or questions\n\
         - User preferences expressed\n\
         \n\
         Output a structured summary, not a conversation replay. Be concise.\n\
         \n\
         Conversation to summarize:\n{conversation_text}"
    );

    // Use the same model to summarize
    let summary_messages = vec![Message::user(summary_prompt)];
    let response = client
        .send_message(
            "You are a conversation summarizer. Output only the summary.",
            &summary_messages,
            &[], // no tools for summarization
        )
        .await?;

    let summary_text = response
        .choices
        .first()
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.content.clone())
        .unwrap_or_else(|| "(compaction failed — no summary generated)".to_string());

    // Build compacted messages: summary + recent
    let summary_msg = Message::user(format!(
        "[Session context — compacted summary of {split_point} messages]\n{summary_text}"
    ));

    let mut compacted = vec![summary_msg];
    compacted.extend(recent_messages.iter().cloned());

    let compacted_count = compacted.len();

    tracing::info!(
        "Compacted: {original_count} → {compacted_count} messages (saved ~{} tokens)",
        estimated - estimate_tokens(&compacted, system_prompt)
    );

    Ok(Some(CompactionResult {
        compacted_messages: compacted,
        original_count,
        compacted_count,
    }))
}

/// Format messages for the summarization prompt
fn format_for_summarization(messages: &[Message]) -> String {
    let mut lines = Vec::new();

    for msg in messages {
        let role = match msg.role {
            crate::llm::types::Role::User => "User",
            crate::llm::types::Role::Assistant => "Assistant",
            crate::llm::types::Role::System => "System",
            crate::llm::types::Role::Tool => "Tool",
        };

        let content = msg.content.as_deref().unwrap_or("");

        // Truncate very long messages (tool results can be huge)
        let truncated = if content.len() > 500 {
            format!("{}... (truncated)", &content[..500])
        } else {
            content.to_string()
        };

        if let Some(ref tool_calls) = msg.tool_calls {
            for tc in tool_calls {
                lines.push(format!("[{role} called tool: {}]", tc.function.name));
            }
        }

        if !truncated.is_empty() {
            lines.push(format!("{role}: {truncated}"));
        }
    }

    lines.join("\n")
}
