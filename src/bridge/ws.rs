use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

use super::server::AppState;
use super::types::*;

/// Handle a WebSocket connection for streaming interaction
pub async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    // Send initial status
    let status = WsServerMessage::Status {
        model: state.model.clone(),
        session_id: uuid::Uuid::new_v4().to_string(),
    };
    if let Ok(json) = serde_json::to_string(&status) {
        let _ = socket.send(Message::Text(json.into())).await;
    }

    // Process messages
    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                let client_msg: WsClientMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        send_error(&mut socket, &format!("Invalid message: {e}")).await;
                        continue;
                    }
                };

                match client_msg {
                    WsClientMessage::Prompt { text } => {
                        handle_prompt(&mut socket, &state, &text).await;
                    }
                    WsClientMessage::Cancel => {
                        send_error(&mut socket, "Cancel not yet implemented").await;
                    }
                    WsClientMessage::Approve { .. } | WsClientMessage::Deny { .. } => {
                        // Tool approval over WebSocket — future enhancement
                        send_error(&mut socket, "Remote tool approval not yet implemented").await;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn handle_prompt(socket: &mut WebSocket, state: &AppState, prompt: &str) {
    let client = match crate::llm::WorkersAIClient::new(
        Some(state.cf_token.clone()),
        Some(state.account_id.clone()),
        state.model.clone(),
    ) {
        Ok(c) => c,
        Err(e) => {
            send_error(socket, &format!("Client error: {e}")).await;
            return;
        }
    };

    let system = format!(
        "You are oxshell, an AI coding assistant. Be concise.\nWorking directory: {}",
        state.cwd
    );
    let messages = vec![crate::llm::types::Message::user(prompt.to_string())];

    // Streaming response
    match client
        .send_message_streaming(&system, &messages, &state.tool_schema)
        .await
    {
        Ok(mut rx) => {
            let mut total_usage = crate::llm::types::Usage::default();

            while let Some(event) = rx.recv().await {
                let ws_msg = match event {
                    crate::llm::types::StreamEvent::TextDelta(text) => {
                        Some(WsServerMessage::TextDelta { text })
                    }
                    crate::llm::types::StreamEvent::ToolCallStart { name, .. } => {
                        Some(WsServerMessage::ToolUse {
                            name,
                            input: String::new(),
                        })
                    }
                    crate::llm::types::StreamEvent::Done(response) => {
                        if let Some(ref usage) = response.usage {
                            total_usage.accumulate(usage);
                        }
                        Some(WsServerMessage::Done {
                            session_id: uuid::Uuid::new_v4().to_string(),
                            usage: UsageInfo {
                                prompt_tokens: total_usage.prompt_tokens,
                                completion_tokens: total_usage.completion_tokens,
                                total_tokens: total_usage.total_tokens,
                            },
                        })
                    }
                    crate::llm::types::StreamEvent::Error(e) => {
                        Some(WsServerMessage::Error { message: e })
                    }
                    _ => None,
                };

                if let Some(msg) = ws_msg {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                }
            }
        }
        Err(e) => {
            send_error(socket, &format!("Streaming error: {e}")).await;
        }
    }
}

async fn send_error(socket: &mut WebSocket, message: &str) {
    let msg = WsServerMessage::Error {
        message: message.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(json.into())).await;
    }
}
