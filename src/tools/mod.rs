pub mod a2e;
pub mod bash;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod glob;
pub mod grep;
pub mod skill;
pub mod task_tools;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::llm::types::ToolDefinition;
use crate::permissions::{PermissionManager, ToolPermission};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;

    fn permission(&self) -> ToolPermission {
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput>;
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn success(content: String) -> Self {
        Self { content, is_error: false }
    }

    pub fn error(message: String) -> Self {
        Self { content: message, is_error: true }
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        registry.register(Box::new(bash::BashTool));
        registry.register(Box::new(file_read::FileReadTool));
        registry.register(Box::new(file_write::FileWriteTool));
        registry.register(Box::new(file_edit::FileEditTool));
        registry.register(Box::new(glob::GlobTool));
        registry.register(Box::new(grep::GrepTool));

        registry
    }

    /// Register the SkillTool with the list of available skill names
    pub fn register_skill_tool(&mut self, skill_names: &[&str]) {
        self.register(Box::new(skill::SkillTool::new(skill_names)));
    }

    fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Register an external tool (from MCP server)
    pub fn register_external(&mut self, tool: Box<dyn Tool>) {
        self.register(tool);
    }

    /// Get OpenAI function calling schema definitions
    pub fn schema(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|t| ToolDefinition {
                tool_type: "function".to_string(),
                function: crate::llm::types::FunctionDefinition {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.input_schema(),
                },
            })
            .collect()
    }

    /// Execute a tool, returning ToolOutput (preserves error flag)
    pub async fn execute(
        &self,
        name: &str,
        input: &Value,
        permissions: &PermissionManager,
    ) -> Result<ToolOutput> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {name}"))?;

        if !permissions.check(name, tool.permission(), input) {
            return Ok(ToolOutput::error(format!("Permission denied for tool: {name}")));
        }

        match tool.execute(input).await {
            Ok(output) => Ok(output),
            Err(e) => {
                tracing::error!("Tool '{name}' failed: {e}");
                Ok(ToolOutput::error(format!("Tool '{name}' error: {e}")))
            }
        }
    }

    /// Get the permission level for a named tool
    pub fn get_permission(&self, name: &str) -> ToolPermission {
        self.tools
            .get(name)
            .map(|t| t.permission())
            .unwrap_or(ToolPermission::RequiresApproval)
    }
}
