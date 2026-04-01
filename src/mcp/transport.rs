use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};

use super::protocol::*;

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Stdio-based MCP transport. Spawns a child process and communicates
/// via newline-delimited JSON-RPC 2.0 over stdin/stdout.
pub struct StdioTransport {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    /// Pending request handlers, keyed by request ID
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Server name for logging
    pub server_name: String,
}

impl StdioTransport {
    /// Spawn a child process and start the reader loop
    pub async fn spawn(
        server_name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // Suppress server stderr
            .kill_on_drop(true);

        // Add environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server '{server_name}': {command}"))?;

        let stdin = child
            .stdin
            .take()
            .context("Failed to capture stdin of MCP server")?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture stdout of MCP server")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Spawn reader task
        let pending_clone = pending.clone();
        let name_clone = server_name.to_string();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                // Parse JSON-RPC response
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(response) => {
                        if let Some(id) = response.id {
                            let mut pending = pending_clone.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                let _ = tx.send(response);
                            }
                        }
                        // Notifications (no id) are ignored for now
                    }
                    Err(e) => {
                        tracing::debug!("[mcp:{name_clone}] Failed to parse response: {e} — line: {line}");
                    }
                }
            }

            tracing::info!("[mcp:{name_clone}] Reader loop ended (server process exited)");
        });

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            server_name: server_name.to_string(),
        })
    }

    /// Send a JSON-RPC request and wait for a response with timeout
    pub async fn request(&self, req: JsonRpcRequest) -> Result<serde_json::Value> {
        let id = req.id;

        // Register pending handler
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // Serialize and send
        let json = serde_json::to_string(&req)?;
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        tracing::debug!("[mcp:{}] → {}", self.server_name, json);

        // Wait for response with timeout
        let response = tokio::time::timeout(REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "MCP server '{}' timed out after {}s for request {}",
                    self.server_name,
                    REQUEST_TIMEOUT.as_secs(),
                    id
                )
            })?
            .map_err(|_| anyhow::anyhow!("MCP response channel dropped"))?;

        // Check for errors
        if let Some(err) = response.error {
            bail!(
                "MCP server '{}' error {}: {}",
                self.server_name,
                err.code,
                err.message
            );
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    /// Send a notification (no response expected)
    pub async fn notify(&self, notification: serde_json::Value) -> Result<()> {
        let json = serde_json::to_string(&notification)?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Initialize the MCP handshake
    pub async fn initialize(&self) -> Result<serde_json::Value> {
        let result = self.request(initialize_request()).await?;
        self.notify(initialized_notification()).await?;
        tracing::info!(
            "[mcp:{}] Initialized (protocol: {})",
            self.server_name,
            result
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        );
        Ok(result)
    }

    /// List available tools from the server
    pub async fn list_tools(&self) -> Result<Vec<MCPToolInfo>> {
        let result = self.request(tools_list_request()).await?;
        let tools = parse_tools_list(&result);
        tracing::info!(
            "[mcp:{}] Discovered {} tools",
            self.server_name,
            tools.len()
        );
        Ok(tools)
    }

    /// Call a tool on the server
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String> {
        let result = self.request(tools_call_request(name, arguments)).await?;

        // Check isError flag
        if result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false) {
            let text = parse_tool_call_result(&result);
            bail!("MCP tool '{name}' returned error: {text}");
        }

        Ok(parse_tool_call_result(&result))
    }

    /// Gracefully shutdown the server
    pub async fn shutdown(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        tracing::debug!("[mcp:{}] Server process killed", self.server_name);
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Kill the child process on drop (best-effort sync)
        // The actual async kill happens in shutdown()
        tracing::debug!("[mcp:{}] Transport dropped", self.server_name);
    }
}
