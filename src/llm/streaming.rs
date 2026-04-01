use anyhow::Result;
use futures_util::StreamExt;
use reqwest::Response;
use tokio::sync::mpsc;

use super::types::*;

const MAX_TOOL_CALLS: usize = 32; // Safety bound

/// Process an SSE stream from Workers AI (OpenAI-compatible format)
pub async fn handle_stream(response: Response, tx: &mpsc::Sender<StreamEvent>) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    let mut full_text = String::new();
    // Use HashMap to handle out-of-order tool call indices safely
    let mut tool_map: std::collections::HashMap<usize, ToolCall> = std::collections::HashMap::new();
    let mut tool_args_map: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut usage = Usage::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            let data = if let Some(d) = line.strip_prefix("data: ") {
                d.trim()
            } else {
                continue;
            };

            if data == "[DONE]" {
                // Merge tool_args into tool_calls (HashMap-based, order-safe)
                let mut final_tool_calls = None;
                if !tool_map.is_empty() {
                    let mut sorted_indices: Vec<usize> = tool_map.keys().copied().collect();
                    sorted_indices.sort();
                    let mut merged = Vec::new();
                    for idx in sorted_indices {
                        if let Some(mut tc) = tool_map.remove(&idx) {
                            if let Some(args) = tool_args_map.remove(&idx) {
                                tc.function.arguments = args;
                            }
                            merged.push(tc);
                        }
                    }
                    final_tool_calls = Some(merged);
                }

                let response = ChatCompletionResponse {
                    id: String::new(),
                    choices: vec![Choice {
                        index: 0,
                        message: Some(ChatMessage {
                            role: Some(Role::Assistant),
                            content: if full_text.is_empty() {
                                None
                            } else {
                                Some(full_text.clone())
                            },
                            tool_calls: final_tool_calls,
                        }),
                        delta: None,
                        finish_reason: Some("stop".into()),
                    }],
                    usage: Some(usage.clone()),
                    model: String::new(),
                };

                let _ = tx.send(StreamEvent::Done(response)).await;
                return Ok(());
            }

            // Parse chunk — log errors instead of silently dropping
            let chunk: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Failed to parse streaming chunk: {e} — data: {data}");
                    continue;
                }
            };

            if let Some(u) = chunk.get("usage") {
                if let Ok(parsed) = serde_json::from_value::<Usage>(u.clone()) {
                    usage.accumulate(&parsed);
                }
            }

            // Check for API error in stream
            if let Some(err) = chunk.get("error").and_then(|e| e.as_str()) {
                let _ = tx
                    .send(StreamEvent::Error(format!("API stream error: {err}")))
                    .await;
                return Ok(());
            }

            if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                for choice in choices {
                    if let Some(delta) = choice.get("delta") {
                        // Text content
                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                full_text.push_str(content);
                                let _ = tx.send(StreamEvent::TextDelta(content.to_string())).await;
                            }
                        }

                        // Tool calls
                        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                            for tc in tcs {
                                let index = tc
                                    .get("index")
                                    .and_then(|i| i.as_u64())
                                    .unwrap_or(0) as usize;

                                // Bounds check — prevent OOM
                                if index >= MAX_TOOL_CALLS {
                                    tracing::warn!(
                                        "Tool call index {index} exceeds max {MAX_TOOL_CALLS}, skipping"
                                    );
                                    continue;
                                }

                                if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                    let name = tc
                                        .get("function")
                                        .and_then(|f| f.get("name"))
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    // HashMap-based: safe for any index order
                                    tool_map.insert(index, ToolCall {
                                        id: id.to_string(),
                                        call_type: "function".into(),
                                        function: FunctionCall {
                                            name: name.clone(),
                                            arguments: String::new(),
                                        },
                                    });
                                    tool_args_map.entry(index).or_default();

                                    let _ = tx
                                        .send(StreamEvent::ToolCallStart {
                                            index: index as u32,
                                            id: id.to_string(),
                                            name,
                                        })
                                        .await;
                                }

                                if let Some(args) = tc
                                    .get("function")
                                    .and_then(|f| f.get("arguments"))
                                    .and_then(|a| a.as_str())
                                {
                                    tool_args_map.entry(index).or_default().push_str(args);
                                    let _ = tx
                                        .send(StreamEvent::ToolCallArgsDelta {
                                            index: index as u32,
                                            args: args.to_string(),
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
