use axum::{
    Json,
    extract::State,
    response::IntoResponse,
    http::StatusCode,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::server::AppState;
use super::types::*;

/// GET /status
pub async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: state.model.clone(),
        session_count: state.active_sessions.load(Ordering::Relaxed),
        memory_count: state.memory_count,
        skills: state.skill_names.clone(),
        tools: state.tool_names.clone(),
        uptime_secs: state.started_at.elapsed().as_secs(),
    })
}

/// POST /prompt — execute a prompt (tools require explicit approval via WebSocket)
pub async fn prompt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PromptRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let session_id = req.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let system = format!(
        "You are oxshell, an AI coding assistant. Be concise and direct.\nWorking directory: {}",
        state.cwd
    );

    let messages = vec![crate::llm::types::Message::user(req.prompt.clone())];

    // Reuse credentials from state — no cloning into new client
    let model = req.model.unwrap_or_else(|| state.model.clone());
    let client = crate::llm::WorkersAIClient::new(
        Some(state.cf_token.clone()),
        Some(state.account_id.clone()),
        model,
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let response = client
        .send_message(&system, &messages, &state.tool_schema)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM error: {e}")))?;

    let choice = response.choices.first()
        .ok_or((StatusCode::BAD_GATEWAY, "No response from model".to_string()))?;
    let msg = choice.message.as_ref()
        .ok_or((StatusCode::BAD_GATEWAY, "Empty response".to_string()))?;

    let response_text = msg.content.clone().unwrap_or_default();
    let usage = response.usage.unwrap_or_default();

    // Tool calls are REPORTED but NOT auto-executed via REST.
    // Use WebSocket for interactive tool approval flow.
    let mut tool_calls_info = Vec::new();
    if let Some(ref tool_calls) = msg.tool_calls {
        for tc in tool_calls {
            tool_calls_info.push(ToolCallInfo {
                name: tc.function.name.clone(),
                result: format!("Tool '{}' requires approval. Use WebSocket /ws for interactive mode.", tc.function.name),
                is_error: false,
            });
        }
    }

    Ok(Json(PromptResponse {
        session_id,
        response: response_text,
        tool_calls: tool_calls_info,
        usage: UsageInfo {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        },
    }))
}

/// GET /tools
pub async fn list_tools(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    Json(state.tool_names.clone())
}

/// GET /skills
pub async fn list_skills(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    Json(state.skill_names.clone())
}

/// GET /sessions
pub async fn list_sessions(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let data_dir = dirs::data_local_dir().unwrap_or_default().join("oxshell");
    let store = crate::session::SessionStore::new(&data_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions = store.recent(20)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result: Vec<serde_json::Value> = sessions.iter().map(|s| {
        serde_json::json!({
            "id": s.id, "title": s.title,
            "created_at": s.created_at.to_rfc3339(),
            "updated_at": s.updated_at.to_rfc3339(),
            "message_count": s.message_count,
        })
    }).collect();

    Ok(Json(result))
}

/// GET /memory
pub async fn memory_stats(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "count": state.memory_count }))
}

/// GET /doctor
pub async fn doctor(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = crate::config::OxshellConfig::load();
    let cwd = std::path::Path::new(&state.cwd);
    let plugins = crate::plugins::PluginRegistry::new(cwd);
    let checks = crate::doctor::run_diagnostics(cwd, &cfg, &plugins, state.memory_count);
    crate::doctor::format_diagnostics(&checks)
}
