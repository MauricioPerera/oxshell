pub mod setup;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent configuration stored at ~/.oxshell/config.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OxshellConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl OxshellConfig {
    /// Config file path: ~/.oxshell/config.json
    pub fn path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".oxshell")
            .join("config.json")
    }

    /// Load config from disk (returns empty config if file doesn't exist)
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json).context("Failed to save config")?;
        Ok(())
    }

    /// Resolve a value: CLI flag > env var > config file
    pub fn resolve_token(&self, cli: &Option<String>) -> Option<String> {
        cli.clone()
            .or_else(|| std::env::var("CLOUDFLARE_API_TOKEN").ok())
            .or_else(|| self.cf_token.clone())
    }

    pub fn resolve_account_id(&self, cli: &Option<String>) -> Option<String> {
        cli.clone()
            .or_else(|| std::env::var("CLOUDFLARE_ACCOUNT_ID").ok())
            .or_else(|| self.account_id.clone())
    }

    pub fn resolve_model(&self, cli: &str) -> String {
        // If CLI has the default value, check config
        if cli == "@hf/nousresearch/hermes-2-pro-mistral-7b" {
            self.model
                .clone()
                .unwrap_or_else(|| cli.to_string())
        } else {
            cli.to_string()
        }
    }

    pub fn is_configured(&self) -> bool {
        self.cf_token.is_some() && self.account_id.is_some()
    }
}
