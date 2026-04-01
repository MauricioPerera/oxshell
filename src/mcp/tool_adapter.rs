use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use super::protocol::MCPToolInfo;
use super::transport::StdioTransport;
use crate::permissions::ToolPermission;
use crate::tools::{Tool, ToolOutput};

/// Adapts an MCP server tool to oxshell's Tool trait.
/// Each MCPToolAdapter wraps one tool from one server.
pub struct MCPToolAdapter {
    /// Prefixed name: "server_name__tool_name"
    pub full_name: String,
    pub tool_info: MCPToolInfo,
    pub server_name: String,
    pub transport: Arc<StdioTransport>,
}

impl MCPToolAdapter {
    pub fn new(
        server_name: &str,
        tool_info: MCPToolInfo,
        transport: Arc<StdioTransport>,
    ) -> Self {
        // Prefix with server name to avoid collisions (like Claude Code's mcp__ prefix)
        let full_name = format!("mcp__{}__{}", server_name, tool_info.name);

        Self {
            full_name,
            tool_info,
            server_name: server_name.to_string(),
            transport,
        }
    }
}

#[async_trait]
impl Tool for MCPToolAdapter {
    fn name(&self) -> &str {
        &self.full_name
    }

    fn description(&self) -> &str {
        &self.tool_info.description
    }

    fn input_schema(&self) -> Value {
        self.tool_info.input_schema.clone()
    }

    fn permission(&self) -> ToolPermission {
        // MCP tools are external — always require approval
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        tracing::info!(
            "[mcp:{}] Calling tool '{}'",
            self.server_name,
            self.tool_info.name
        );

        match self
            .transport
            .call_tool(&self.tool_info.name, input)
            .await
        {
            Ok(result) => Ok(ToolOutput::success(result)),
            Err(e) => {
                tracing::error!(
                    "[mcp:{}] Tool '{}' failed: {e}",
                    self.server_name,
                    self.tool_info.name
                );
                Ok(ToolOutput::error(format!(
                    "MCP tool '{}' error: {e}",
                    self.tool_info.name
                )))
            }
        }
    }
}
