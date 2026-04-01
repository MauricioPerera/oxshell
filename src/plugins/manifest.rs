use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin manifest (plugin.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (unique identifier)
    pub name: String,
    /// Semver version
    #[serde(default = "default_version")]
    pub version: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Author name or organization
    #[serde(default)]
    pub author: String,
    /// Plugin components
    #[serde(default)]
    pub components: PluginComponents,
    /// Required other plugins
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Minimum oxshell version
    #[serde(default)]
    pub min_oxshell_version: Option<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// What a plugin provides
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginComponents {
    /// Custom skills (relative paths to SKILL.md files)
    #[serde(default)]
    pub skills: Vec<String>,
    /// Custom slash commands (name → description)
    #[serde(default)]
    pub commands: HashMap<String, String>,
    /// Agent definitions (relative paths to .md files)
    #[serde(default)]
    pub agents: Vec<String>,
    /// Hook configurations
    #[serde(default)]
    pub hooks: Vec<crate::hooks::types::HookConfig>,
    /// MCP server configurations
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// MCP server config within a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl PluginManifest {
    /// Parse from JSON string
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid plugin.json: {e}"))
    }

    /// Validate manifest fields
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Plugin name is required".to_string());
        }
        if self.name.contains(' ') || self.name.contains('/') {
            return Err("Plugin name cannot contain spaces or slashes".to_string());
        }
        // Validate semver (basic check)
        let parts: Vec<&str> = self.version.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.parse::<u32>().is_err()) {
            return Err(format!("Invalid version '{}': expected semver (x.y.z)", self.version));
        }
        Ok(())
    }
}
