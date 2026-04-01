use std::path::Path;

/// Diagnostic check result
#[derive(Debug)]
pub struct DiagCheck {
    pub name: String,
    pub status: DiagStatus,
    pub detail: String,
}

#[derive(Debug, PartialEq)]
pub enum DiagStatus {
    Ok,
    Warning,
    Error,
}

impl DiagStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Ok => "[ok]",
            Self::Warning => "[warn]",
            Self::Error => "[ERROR]",
        }
    }
}

/// Run all diagnostic checks and return results
pub fn run_diagnostics(
    cwd: &Path,
    config: &crate::config::OxshellConfig,
    plugins: &crate::plugins::PluginRegistry,
    memory_count: usize,
) -> Vec<DiagCheck> {
    let mut checks = Vec::new();

    // System checks
    checks.push(check_platform());
    checks.push(check_shell());
    checks.push(check_rust_version());

    // Config checks
    checks.push(check_config(config));
    checks.push(check_config_file_permissions());

    // Credentials
    checks.push(check_credentials(config));

    // Working directory
    checks.push(check_cwd(cwd));
    checks.push(check_gitignore(cwd));

    // Memory
    checks.push(check_memory(memory_count));

    // Plugins
    checks.push(check_plugins(plugins));

    // MCP config
    checks.push(check_mcp_config(cwd));

    // Skills
    checks.push(check_skills(cwd));

    // Sessions directory
    checks.push(check_sessions());

    checks
}

/// Format diagnostics for display
pub fn format_diagnostics(checks: &[DiagCheck]) -> String {
    let ok = checks.iter().filter(|c| c.status == DiagStatus::Ok).count();
    let warn = checks.iter().filter(|c| c.status == DiagStatus::Warning).count();
    let err = checks.iter().filter(|c| c.status == DiagStatus::Error).count();

    let mut lines = vec![format!("oxshell doctor — {ok} ok, {warn} warnings, {err} errors\n")];

    for check in checks {
        lines.push(format!("  {} {} — {}", check.status.icon(), check.name, check.detail));
    }

    lines.join("\n")
}

// ─── Individual Checks ──────────────────────────────────

fn check_platform() -> DiagCheck {
    DiagCheck {
        name: "Platform".into(),
        status: DiagStatus::Ok,
        detail: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

fn check_shell() -> DiagCheck {
    let shell = if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    };
    DiagCheck {
        name: "Shell".into(),
        status: DiagStatus::Ok,
        detail: shell,
    }
}

fn check_rust_version() -> DiagCheck {
    DiagCheck {
        name: "Version".into(),
        status: DiagStatus::Ok,
        detail: format!("oxshell v{}", env!("CARGO_PKG_VERSION")),
    }
}

fn check_config(config: &crate::config::OxshellConfig) -> DiagCheck {
    if config.is_configured() {
        DiagCheck {
            name: "Config".into(),
            status: DiagStatus::Ok,
            detail: format!("Loaded from {}", crate::config::OxshellConfig::path().display()),
        }
    } else {
        DiagCheck {
            name: "Config".into(),
            status: DiagStatus::Error,
            detail: "Not configured. Run: oxshell setup".into(),
        }
    }
}

fn check_config_file_permissions() -> DiagCheck {
    let path = crate::config::OxshellConfig::path();
    if !path.exists() {
        return DiagCheck {
            name: "Config permissions".into(),
            status: DiagStatus::Warning,
            detail: "Config file not found".into(),
        };
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode > 0o600 {
                return DiagCheck {
                    name: "Config permissions".into(),
                    status: DiagStatus::Warning,
                    detail: format!("File permissions too open ({:o}). Recommended: 600", mode),
                };
            }
        }
    }

    DiagCheck {
        name: "Config permissions".into(),
        status: DiagStatus::Ok,
        detail: "Permissions OK".into(),
    }
}

fn check_credentials(config: &crate::config::OxshellConfig) -> DiagCheck {
    let has_token = config.cf_token.is_some()
        || std::env::var("CLOUDFLARE_API_TOKEN").is_ok();
    let has_account = config.account_id.is_some()
        || std::env::var("CLOUDFLARE_ACCOUNT_ID").is_ok();

    if has_token && has_account {
        DiagCheck {
            name: "Credentials".into(),
            status: DiagStatus::Ok,
            detail: "Cloudflare token + account ID configured".into(),
        }
    } else {
        let missing: Vec<&str> = [
            if !has_token { Some("API token") } else { None },
            if !has_account { Some("Account ID") } else { None },
        ]
        .iter()
        .filter_map(|x| *x)
        .collect();

        DiagCheck {
            name: "Credentials".into(),
            status: DiagStatus::Error,
            detail: format!("Missing: {}", missing.join(", ")),
        }
    }
}

fn check_cwd(cwd: &Path) -> DiagCheck {
    if cwd.exists() && cwd.is_dir() {
        let is_git = cwd.join(".git").exists();
        DiagCheck {
            name: "Working directory".into(),
            status: DiagStatus::Ok,
            detail: format!("{}{}", cwd.display(), if is_git { " (git repo)" } else { "" }),
        }
    } else {
        DiagCheck {
            name: "Working directory".into(),
            status: DiagStatus::Error,
            detail: format!("{} does not exist", cwd.display()),
        }
    }
}

fn check_gitignore(cwd: &Path) -> DiagCheck {
    let gitignore = cwd.join(".gitignore");
    if !cwd.join(".git").exists() {
        return DiagCheck {
            name: "Gitignore".into(),
            status: DiagStatus::Ok,
            detail: "Not a git repo (skipped)".into(),
        };
    }
    if gitignore.exists() {
        if let Ok(content) = std::fs::read_to_string(&gitignore) {
            if content.contains(".oxshell") || content.contains("*.mmdb") {
                return DiagCheck {
                    name: "Gitignore".into(),
                    status: DiagStatus::Ok,
                    detail: "oxshell files excluded".into(),
                };
            }
        }
        DiagCheck {
            name: "Gitignore".into(),
            status: DiagStatus::Warning,
            detail: "Consider adding .oxshell/ and *.mmdb to .gitignore".into(),
        }
    } else {
        DiagCheck {
            name: "Gitignore".into(),
            status: DiagStatus::Warning,
            detail: "No .gitignore found".into(),
        }
    }
}

fn check_memory(count: usize) -> DiagCheck {
    let status = if count > 400 {
        DiagStatus::Warning
    } else {
        DiagStatus::Ok
    };
    DiagCheck {
        name: "Memory".into(),
        status,
        detail: format!("{count} entries{}", if count > 400 { " (consider consolidation)" } else { "" }),
    }
}

fn check_plugins(plugins: &crate::plugins::PluginRegistry) -> DiagCheck {
    let all = plugins.all();
    let errors: Vec<&str> = all
        .iter()
        .filter(|p| !p.enabled)
        .map(|p| p.manifest.name.as_str())
        .collect();

    if errors.is_empty() {
        DiagCheck {
            name: "Plugins".into(),
            status: DiagStatus::Ok,
            detail: format!("{} active", plugins.active_count()),
        }
    } else {
        DiagCheck {
            name: "Plugins".into(),
            status: DiagStatus::Warning,
            detail: format!(
                "{} active, {} with errors: {}",
                plugins.active_count(),
                errors.len(),
                errors.join(", ")
            ),
        }
    }
}

fn check_mcp_config(cwd: &Path) -> DiagCheck {
    let candidates = [
        cwd.join(".oxshell/mcp.json"),
        cwd.join(".claude/mcp.json"),
    ];
    for path in &candidates {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if serde_json::from_str::<serde_json::Value>(&content).is_ok() {
                    return DiagCheck {
                        name: "MCP config".into(),
                        status: DiagStatus::Ok,
                        detail: format!("Valid: {}", path.display()),
                    };
                } else {
                    return DiagCheck {
                        name: "MCP config".into(),
                        status: DiagStatus::Error,
                        detail: format!("Invalid JSON: {}", path.display()),
                    };
                }
            }
        }
    }
    DiagCheck {
        name: "MCP config".into(),
        status: DiagStatus::Ok,
        detail: "No MCP servers configured".into(),
    }
}

fn check_skills(cwd: &Path) -> DiagCheck {
    let dirs = [
        cwd.join(".oxshell/skills"),
        cwd.join(".claude/skills"),
    ];
    let mut count = 0;
    for dir in &dirs {
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                count += entries
                    .flatten()
                    .filter(|e| e.path().join("SKILL.md").exists())
                    .count();
            }
        }
    }
    DiagCheck {
        name: "Skills".into(),
        status: DiagStatus::Ok,
        detail: format!("{count} custom skills found"),
    }
}

fn check_sessions() -> DiagCheck {
    let sessions_dir = dirs::data_local_dir()
        .unwrap_or_default()
        .join("oxshell/sessions");
    if sessions_dir.exists() {
        let index = sessions_dir.join("index.json");
        if index.exists() {
            if let Ok(content) = std::fs::read_to_string(&index) {
                let count = serde_json::from_str::<Vec<serde_json::Value>>(&content)
                    .map(|v| v.len())
                    .unwrap_or(0);
                return DiagCheck {
                    name: "Sessions".into(),
                    status: DiagStatus::Ok,
                    detail: format!("{count} sessions stored"),
                };
            }
        }
    }
    DiagCheck {
        name: "Sessions".into(),
        status: DiagStatus::Ok,
        detail: "No sessions yet".into(),
    }
}
