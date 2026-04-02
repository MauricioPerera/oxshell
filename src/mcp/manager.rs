use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::tool_adapter::MCPToolAdapter;
use super::transport::StdioTransport;
use crate::tools::ToolRegistry;

/// MCP server configuration from .oxshell/mcp.json
#[derive(Debug, Deserialize)]
pub struct MCPConfig {
    #[serde(default)]
    pub servers: HashMap<String, ServerConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Manages all MCP server connections.
/// Spawns servers, discovers tools, and registers them into the ToolRegistry.
pub struct MCPManager {
    transports: Vec<Arc<StdioTransport>>,
}

impl MCPManager {
    /// Load config, spawn servers, discover tools, register into registry
    pub async fn init(cwd: &Path, registry: &mut ToolRegistry) -> Result<Self> {
        let config = Self::load_config(cwd)?;

        if config.servers.is_empty() {
            return Ok(Self {
                transports: Vec::new(),
            });
        }

        let mut transports = Vec::new();
        let mut total_tools = 0;

        for (name, server) in &config.servers {
            match Self::connect_server(name, server).await {
                Ok((transport, tools)) => {
                    let transport = Arc::new(transport);

                    for tool_info in tools {
                        let adapter = MCPToolAdapter::new(name, tool_info, transport.clone());
                        tracing::debug!(
                            "[mcp:{}] Registered tool: {}",
                            name,
                            adapter.full_name
                        );
                        registry.register_external(Box::new(adapter));
                        total_tools += 1;
                    }

                    transports.push(transport);
                }
                Err(e) => {
                    tracing::error!("[mcp:{name}] Failed to connect: {e}");
                    // Continue with other servers — don't fail entirely
                }
            }
        }

        if total_tools > 0 {
            tracing::info!(
                "MCP: {} servers connected, {} tools registered",
                transports.len(),
                total_tools
            );
        }

        Ok(Self { transports })
    }

    /// Load .oxshell/mcp.json configuration
    fn load_config(cwd: &Path) -> Result<MCPConfig> {
        // Search in multiple locations
        let candidates = [
            cwd.join(".oxshell/mcp.json"),
            cwd.join(".claude/mcp.json"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".oxshell/mcp.json"),
        ];

        for path in &candidates {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let config: MCPConfig = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", path.display()))?;
                tracing::info!(
                    "MCP config loaded from {} ({} servers)",
                    path.display(),
                    config.servers.len()
                );
                return Ok(config);
            }
        }

        // No config found — return empty
        Ok(MCPConfig {
            servers: HashMap::new(),
        })
    }

    /// Connect to a single MCP server: spawn, initialize, list tools
    async fn connect_server(
        name: &str,
        config: &ServerConfig,
    ) -> Result<(StdioTransport, Vec<super::protocol::MCPToolInfo>)> {
        tracing::info!(
            "[mcp:{name}] Connecting: {} {}",
            config.command,
            config.args.join(" ")
        );

        let transport =
            StdioTransport::spawn(name, &config.command, &config.args, &config.env).await?;

        // Initialize handshake
        transport.initialize().await?;

        // Discover tools
        let tools = transport.list_tools().await?;

        for tool in &tools {
            tracing::info!("[mcp:{name}] Tool: {} — {}", tool.name, tool.description);
        }

        Ok((transport, tools))
    }

    /// Gracefully shutdown all servers
    pub async fn shutdown(&self) {
        for transport in &self.transports {
            transport.shutdown().await;
        }
    }

    /// Number of connected servers
    #[allow(dead_code)]
    pub fn server_count(&self) -> usize {
        self.transports.len()
    }
}
