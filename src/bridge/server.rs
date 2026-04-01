use anyhow::Result;
use axum::{
    Router,
    extract::WebSocketUpgrade,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tower_http::cors::{CorsLayer, AllowOrigin};

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
    /// Cached reqwest client (reuse connections)
    pub http_client: reqwest::Client,
    /// Active session count
    pub active_sessions: AtomicUsize,
}

/// HTTP + WebSocket bridge server
pub struct BridgeServer;

impl BridgeServer {
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
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            active_sessions: AtomicUsize::new(0),
        });

        // Restrictive CORS — localhost only
        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(|origin, _| {
                let host = origin.as_bytes();
                host.starts_with(b"http://localhost")
                    || host.starts_with(b"http://127.0.0.1")
                    || host.starts_with(b"http://[::1]")
            }))
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
            ])
            .allow_headers([axum::http::header::CONTENT_TYPE]);

        let app = Router::new()
            .route("/status", get(routes::status))
            .route("/prompt", post(routes::prompt))
            .route("/tools", get(routes::list_tools))
            .route("/skills", get(routes::list_skills))
            .route("/sessions", get(routes::list_sessions))
            .route("/memory", get(routes::memory_stats))
            .route("/doctor", get(routes::doctor))
            .route("/ws", get(ws_handler))
            .layer(cors)
            .with_state(state);

        let addr = format!("127.0.0.1:{port}");
        println!("oxshell bridge server running at http://{addr}");
        println!();
        println!("  REST API:");
        println!("    GET  /status    — Server info");
        println!("    POST /prompt    — Execute prompt (JSON: {{\"prompt\":\"...\"}})");
        println!("    GET  /tools     — List tools");
        println!("    GET  /skills    — List skills");
        println!("    GET  /sessions  — Recent sessions");
        println!("    GET  /memory    — Memory stats");
        println!("    GET  /doctor    — Run diagnostics");
        println!();
        println!("  WebSocket: ws://{addr}/ws");
        println!("  CORS: localhost only");
        println!("  Model: {model}");
        println!();
        println!("  Press Ctrl+C to stop");

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    state.active_sessions.fetch_add(1, Ordering::Relaxed);
    let state_clone = state.clone();
    ws.on_upgrade(move |socket| async move {
        ws::handle_ws(socket, state_clone.clone()).await;
        state_clone.active_sessions.fetch_sub(1, Ordering::Relaxed);
    })
}
