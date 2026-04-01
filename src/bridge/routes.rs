use axum::{
    Json,
    extract::State,
    response::IntoResponse,
    http::StatusCode,
};
use std::sync::Arc;

use super::server::AppState;
use super::types::*;

/// GET /status — server health and info
pub async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let uptime = state.started_at.elapsed().as_secs();
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: state.model.clone(),
        session_count: 0, // TODO: track active sessions
        memory_count: state.memory_count,
        skills: state.skill_names.clone(),
        tools: state.tool_names.clone(),
        uptime_secs: uptime,
    })
}

/// POST /prompt — execute a prompt and return result
pub async fn prompt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PromptRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let session_id = req
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Build system prompt
    let system = format!(
        "You are oxshell, an AI coding assistant. Be concise and direct.\nWorking directory: {}",
        state.cwd
    );

    // Create messages
    let messages = vec![crate::llm::types::Message::user(req.prompt.clone())];

    // Send to Workers AI
    let client = match crate::llm::WorkersAIClient::new(
        Some(state.cf_token.clone()),
        Some(state.account_id.clone()),
        req.model.unwrap_or_else(|| state.model.clone()),
    ) {
        Ok(c) => c,
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };

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

    // Handle tool calls if present and auto_approve is set
    let mut tool_results = Vec::new();
    if req.auto_approve {
        if let Some(ref tool_calls) = msg.tool_calls {
            let tools = crate::tools::ToolRegistry::new();
            let permissions = crate::permissions::PermissionManager::new(true);

            for tc in tool_calls {
                let input = tc.function.parse_arguments();
                let output = tools.execute(&tc.function.name, &input, &permissions)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                tool_results.push(ToolCallInfo {
                    name: tc.function.name.clone(),
                    result: output.content,
                    is_error: output.is_error,
                });
            }
        }
    }

    Ok(Json(PromptResponse {
        session_id,
        response: response_text,
        tool_calls: tool_results,
        usage: UsageInfo {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        },
    }))
}

/// GET /tools — list available tools
pub async fn list_tools(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    Json(state.tool_names.clone())
}

/// GET /skills — list available skills
pub async fn list_skills(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    Json(state.skill_names.clone())
}

/// GET /sessions — list recent sessions
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_default()
        .join("oxshell");
    let store = crate::session::SessionStore::new(&data_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions = store.recent(20)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result: Vec<serde_json::Value> = sessions.iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "title": s.title,
            "created_at": s.created_at.to_rfc3339(),
            "updated_at": s.updated_at.to_rfc3339(),
            "message_count": s.message_count,
            "model": s.model,
        })
    }).collect();

    Ok(Json(result))
}

/// GET /memory — memory stats
pub async fn memory_stats(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "count": state.memory_count,
    }))
}

/// GET /doctor — run diagnostics
pub async fn doctor(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = crate::config::OxshellConfig::load();
    let cwd = std::path::Path::new(&state.cwd);
    let plugins = crate::plugins::PluginRegistry::new(cwd);
    let checks = crate::doctor::run_diagnostics(cwd, &cfg, &plugins, state.memory_count);
    crate::doctor::format_diagnostics(&checks)
}
