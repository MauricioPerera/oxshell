use std::path::Path;

use super::loader::{LoadedPlugin, PluginSource, scan_plugins_dir};

/// Registry of all discovered plugins.
/// Scans project-level and user-level plugin directories.
pub struct PluginRegistry {
    plugins: Vec<LoadedPlugin>,
}

impl PluginRegistry {
    /// Discover and load all plugins from known directories
    pub fn new(cwd: &Path) -> Self {
        let mut plugins = Vec::new();

        // Project plugins: .oxshell/plugins/
        let project_dir = cwd.join(".oxshell").join("plugins");
        if project_dir.exists() {
            plugins.extend(scan_plugins_dir(&project_dir, PluginSource::Local));
        }

        // Also check .claude/plugins/ for compatibility
        let claude_dir = cwd.join(".claude").join("plugins");
        if claude_dir.exists() {
            plugins.extend(scan_plugins_dir(&claude_dir, PluginSource::Local));
        }

        // User plugins: ~/.oxshell/plugins/
        if let Some(home) = dirs::home_dir() {
            let user_dir = home.join(".oxshell").join("plugins");
            if user_dir.exists() {
                plugins.extend(scan_plugins_dir(&user_dir, PluginSource::User));
            }
        }

        // Deduplicate by name (project-level wins over user-level)
        let mut seen = std::collections::HashSet::new();
        plugins.retain(|p| seen.insert(p.manifest.name.clone()));

        let active = plugins.iter().filter(|p| p.enabled).count();
        let total = plugins.len();
        if total > 0 {
            tracing::info!("Plugins: {active} active, {} with errors, {total} total", total - active);
        }

        Self { plugins }
    }

    /// Get all loaded plugins
    pub fn all(&self) -> &[LoadedPlugin] {
        &self.plugins
    }

    /// Get only enabled plugins
    pub fn enabled(&self) -> Vec<&LoadedPlugin> {
        self.plugins.iter().filter(|p| p.enabled).collect()
    }

    /// Get plugin by name
    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.iter().find(|p| p.manifest.name == name)
    }

    /// Collect all skill paths from enabled plugins
    pub fn skill_paths(&self) -> Vec<std::path::PathBuf> {
        self.enabled()
            .iter()
            .flat_map(|p| {
                p.manifest.components.skills.iter().map(move |s| p.dir.join(s))
            })
            .filter(|p| p.exists())
            .collect()
    }

    /// Collect all hook configs from enabled plugins
    pub fn hook_configs(&self) -> Vec<crate::hooks::types::HookConfig> {
        self.enabled()
            .iter()
            .flat_map(|p| p.manifest.components.hooks.clone())
            .collect()
    }

    /// Collect all MCP server configs from enabled plugins
    pub fn mcp_configs(&self) -> std::collections::HashMap<String, super::manifest::McpServerConfig> {
        let mut configs = std::collections::HashMap::new();
        for plugin in self.enabled() {
            for (name, config) in &plugin.manifest.components.mcp_servers {
                let prefixed = format!("{}:{}", plugin.manifest.name, name);
                configs.insert(prefixed, config.clone());
            }
        }
        configs
    }

    /// Count of active plugins
    pub fn active_count(&self) -> usize {
        self.plugins.iter().filter(|p| p.enabled).count()
    }

    /// Format plugin list for display
    pub fn format_list(&self) -> String {
        if self.plugins.is_empty() {
            return "No plugins installed.\nAdd plugins to .oxshell/plugins/<name>/plugin.json".to_string();
        }

        let mut lines = Vec::new();
        for p in &self.plugins {
            let status = if p.enabled { "active" } else { "error" };
            let source = match p.source {
                PluginSource::Local => "project",
                PluginSource::User => "user",
            };
            lines.push(format!(
                "  {} v{} [{}] ({}) — {}",
                p.manifest.name,
                p.manifest.version,
                status,
                source,
                p.manifest.description
            ));
            for err in &p.errors {
                lines.push(format!("    ! {err}"));
            }
        }
        lines.join("\n")
    }
}
