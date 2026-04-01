use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::manifest::PluginManifest;

/// A loaded plugin with its manifest and resolved paths
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
    pub enabled: bool,
    pub source: PluginSource,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    /// Loaded from local directory
    Local,
    /// Loaded from user's global plugins
    User,
}

/// Load a plugin from a directory containing plugin.json
pub fn load_plugin(dir: &Path, source: PluginSource) -> Result<LoadedPlugin> {
    let manifest_path = dir.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!("No plugin.json found in {}", dir.display());
    }

    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let manifest = PluginManifest::parse(&content)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut errors = Vec::new();

    // Validate manifest
    if let Err(e) = manifest.validate() {
        errors.push(format!("Manifest validation: {e}"));
    }

    // Validate skill paths exist
    for skill_path in &manifest.components.skills {
        let full = dir.join(skill_path);
        if !full.exists() {
            errors.push(format!("Skill not found: {skill_path}"));
        }
    }

    // Validate agent paths exist
    for agent_path in &manifest.components.agents {
        let full = dir.join(agent_path);
        if !full.exists() {
            errors.push(format!("Agent not found: {agent_path}"));
        }
    }

    Ok(LoadedPlugin {
        manifest,
        dir: dir.to_path_buf(),
        enabled: errors.is_empty(),
        source,
        errors,
    })
}

/// Scan a directory for plugin subdirectories
pub fn scan_plugins_dir(dir: &Path, source: PluginSource) -> Vec<LoadedPlugin> {
    let mut plugins = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return plugins,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("plugin.json").exists() {
            continue;
        }

        match load_plugin(&path, source.clone()) {
            Ok(plugin) => {
                if !plugin.errors.is_empty() {
                    tracing::warn!(
                        "Plugin '{}' has errors: {}",
                        plugin.manifest.name,
                        plugin.errors.join(", ")
                    );
                }
                plugins.push(plugin);
            }
            Err(e) => {
                tracing::warn!("Failed to load plugin at {}: {e}", path.display());
            }
        }
    }

    plugins
}
