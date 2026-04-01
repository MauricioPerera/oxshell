use anyhow::Result;
use axum::{
    Router,
    extract::WebSocketUpgrade,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;

use super::{routes, ws};

/// Shared state for the bridge server
pub struct AppState {
    pub cf_token: String,
    pub account_id: String,
    pub model: String,
    pub cwd: String,
    pub memory_count: usize,
    pub skill_names: Vec<String>,
    pub tool_names: Vec<String>,
    pub tool_schema: Vec<crate::llm::types::ToolDefinition>,
    pub started_at: Instant,
}

/// HTTP + WebSocket bridge server
pub struct BridgeServer;

impl BridgeServer {
    /// Start the bridge server on the given port
    pub async fn start(
        port: u16,
        cf_token: String,
        account_id: String,
        model: String,
        cwd: String,
        memory_count: usize,
        skill_names: Vec<String>,
        tool_names: Vec<String>,
        tool_schema: Vec<crate::llm::types::ToolDefinition>,
    ) -> Result<()> {
        let state = Arc::new(AppState {
            cf_token,
            account_id,
            model: model.clone(),
            cwd,
            memory_count,
            skill_names,
            tool_names,
            tool_schema,
            started_at: Instant::now(),
        });

        let app = Router::new()
            // REST API
            .route("/status", get(routes::status))
            .route("/prompt", post(routes::prompt))
            .route("/tools", get(routes::list_tools))
            .route("/skills", get(routes::list_skills))
            .route("/sessions", get(routes::list_sessions))
            .route("/memory", get(routes::memory_stats))
            .route("/doctor", get(routes::doctor))
            // WebSocket
            .route("/ws", get(ws_handler))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = format!("127.0.0.1:{port}");
        println!("oxshell bridge server running at http://{addr}");
        println!();
        println!("  REST API:");
        println!("    GET  /status    — Server info");
        println!("    POST /prompt    — Execute prompt (JSON body)");
        println!("    GET  /tools     — List tools");
        println!("    GET  /skills    — List skills");
        println!("    GET  /sessions  — Recent sessions");
        println!("    GET  /memory    — Memory stats");
        println!("    GET  /doctor    — Run diagnostics");
        println!();
        println!("  WebSocket:");
        println!("    ws://{addr}/ws  — Streaming interaction");
        println!();
        println!("  Model: {model}");
        println!("  Press Ctrl+C to stop");

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// WebSocket upgrade handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws::handle_ws(socket, state))
}
