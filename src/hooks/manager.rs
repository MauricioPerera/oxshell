use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

use super::types::*;

/// Manages hook registration and execution.
/// Hooks are loaded from .oxshell/hooks.json and can be added at runtime.
pub struct HookManager {
    hooks: Vec<HookConfig>,
}

impl HookManager {
    /// Load hooks from config file, or empty if none exists
    pub fn new(cwd: &Path) -> Self {
        let mut hooks = Vec::new();

        // Load from .oxshell/hooks.json
        let candidates = [
            cwd.join(".oxshell/hooks.json"),
            cwd.join(".claude/hooks.json"),
        ];

        for path in &candidates {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(settings) = serde_json::from_str::<HooksSettings>(&content) {
                        hooks.extend(settings.hooks);
                        tracing::info!("Loaded {} hooks from {}", hooks.len(), path.display());
                    } else {
                        tracing::warn!("Invalid hooks config at {}", path.display());
                    }
                }
            }
        }

        Self { hooks }
    }

    /// Add a hook at runtime (session-scoped)
    pub fn add(&mut self, config: HookConfig) {
        self.hooks.push(config);
    }

    /// Run all hooks matching an event. Returns the combined action.
    /// - If any hook returns Block, the entire action is blocked.
    /// - If any hook returns Modify, the last modification wins.
    /// - Otherwise, Allow.
    pub async fn run(
        &self,
        event: HookEvent,
        tool_name: Option<&str>,
        context: &HashMap<String, String>,
    ) -> HookAction {
        let matching: Vec<&HookConfig> = self
            .hooks
            .iter()
            .filter(|h| {
                if h.event != event {
                    return false;
                }
                // If hook has matcher, check tool name
                if let Some(ref matcher) = h.matcher {
                    if let Some(name) = tool_name {
                        return name == matcher || matcher == "*";
                    }
                    return false;
                }
                true // No matcher = matches all
            })
            .collect();

        if matching.is_empty() {
            return HookAction::Allow;
        }

        let mut last_action = HookAction::Allow;

        for hook in matching {
            match execute_hook(hook, context).await {
                Ok(action) => {
                    match &action {
                        HookAction::Block(reason) => {
                            tracing::info!(
                                "Hook blocked {}: {}",
                                event.as_str(),
                                reason
                            );
                            return action; // Block immediately
                        }
                        HookAction::Modify(_) | HookAction::Allow => {
                            last_action = action;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Hook for {} failed: {e}", event.as_str());
                    // Hook failure doesn't block the operation
                }
            }
        }

        last_action
    }

    /// Check if any hooks are registered for an event
    pub fn has_hooks(&self, event: HookEvent) -> bool {
        self.hooks.iter().any(|h| h.event == event)
    }

    pub fn count(&self) -> usize {
        self.hooks.len()
    }
}

/// Execute a single hook and return its action
async fn execute_hook(
    hook: &HookConfig,
    context: &HashMap<String, String>,
) -> Result<HookAction> {
    let command = hook
        .command
        .as_deref()
        .or(hook.script.as_deref())
        .ok_or_else(|| anyhow::anyhow!("Hook has no command or script"))?;

    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("bash", "-c")
    };

    let mut cmd = Command::new(shell);
    cmd.arg(flag).arg(command);

    // Pass context as environment variables
    for (key, value) in context {
        cmd.env(format!("OXSHELL_{}", key.to_uppercase()), value);
    }

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(hook.timeout_ms),
        cmd.output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            match exit_code {
                0 => {
                    if stdout.is_empty() {
                        Ok(HookAction::Allow)
                    } else {
                        Ok(HookAction::Modify(stdout))
                    }
                }
                1 => {
                    // Exit code 1 = block
                    let reason = if stdout.is_empty() {
                        "Hook returned exit code 1".to_string()
                    } else {
                        stdout
                    };
                    Ok(HookAction::Block(reason))
                }
                _ => {
                    Ok(HookAction::Allow) // Other codes = allow but log
                }
            }
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Hook command failed: {e}")),
        Err(_) => Err(anyhow::anyhow!(
            "Hook timed out after {}ms",
            hook.timeout_ms
        )),
    }
}
