use anyhow::{Context, Result, bail};
use std::io::{self, Write};
use std::path::Path;

use super::OxshellConfig;

/// Run the interactive setup/onboarding wizard
pub async fn run_setup() -> Result<()> {
    println!();
    println!("  oxshell setup");
    println!("  =============");
    println!();

    let mut config = OxshellConfig::load();

    // ─── Step 1: Cloudflare API Token ───────────────────
    println!("  Step 1/7: Cloudflare API Token");
    println!();
    println!("  You need a Cloudflare API token with Workers AI permissions.");
    println!("  Create one at: https://dash.cloudflare.com/profile/api-tokens");
    println!();
    println!("  Click 'Create Token' > 'Custom token' with permissions:");
    println!("    - Account > Workers AI > Read");
    println!();

    let token = if let Some(ref existing) = config.cf_token {
        let masked = mask_token(existing);
        let input = prompt(&format!("  API Token [{}]: ", masked))?;
        if input.is_empty() { existing.clone() } else { input }
    } else {
        let input = prompt("  API Token: ")?;
        if input.is_empty() {
            bail!("API token is required. Get one at https://dash.cloudflare.com/profile/api-tokens");
        }
        input
    };

    // ─── Step 2: Account ID ─────────────────────────────
    println!();
    println!("  Step 2/7: Cloudflare Account ID");
    println!();
    println!("  Detecting your accounts...");

    let accounts = detect_accounts(&token).await?;

    let account_id = if accounts.is_empty() {
        println!("  Could not auto-detect accounts.");
        println!("  Find your Account ID at: https://dash.cloudflare.com");
        let input = prompt("  Account ID: ")?;
        if input.is_empty() { bail!("Account ID is required"); }
        input
    } else if accounts.len() == 1 {
        let (id, name) = &accounts[0];
        println!("  Found: {} ({})", name, id);
        let input = prompt("  Use this account? [Y/n]: ")?;
        if input.to_lowercase() == "n" {
            prompt("  Account ID: ")?.to_string()
        } else {
            id.clone()
        }
    } else {
        println!("  Found {} accounts:", accounts.len());
        for (i, (id, name)) in accounts.iter().enumerate() {
            println!("    {}. {} ({})", i + 1, name, id);
        }
        let input = prompt(&format!("  Select [1-{}]: ", accounts.len()))?;
        let idx: usize = input.parse().unwrap_or(1);
        accounts[idx.saturating_sub(1).min(accounts.len() - 1)].0.clone()
    };

    // ─── Step 3: Connection Test ────────────────────────
    println!();
    println!("  Step 3/7: Testing connection...");

    match test_connection(&token, &account_id).await {
        Ok(model_count) => println!("  Connected! {} AI models available.", model_count),
        Err(e) => {
            println!("  Warning: Connection test failed: {e}");
            println!("  Config will be saved anyway — you can fix it later.");
        }
    }

    // ─── Step 4: Model Selection ────────────────────────
    println!();
    println!("  Step 4/7: Default model");
    println!();
    println!("  Recommended models:");
    println!("    1. @hf/nousresearch/hermes-2-pro-mistral-7b  (fast, free tier)");
    println!("    2. @cf/ibm-granite/granite-4.0-h-micro       (131K context, best quality)");
    println!("    3. @cf/meta/llama-4-scout-17b-16e-instruct    (newest, 17B params)");
    println!();

    let model = if let Some(ref existing) = config.model {
        let input = prompt(&format!("  Model [{}]: ", existing))?;
        if input.is_empty() { existing.clone() } else { resolve_model_shorthand(&input) }
    } else {
        let input = prompt("  Select [1-3] or model name [default: 1]: ")?;
        resolve_model_shorthand(&input)
    };

    // ─── Step 5: Theme Selection ────────────────────────
    println!();
    println!("  Step 5/7: Color theme");
    println!();
    println!("    1. dark       (default — orange on dark)");
    println!("    2. light      (dark text on light bg)");
    println!("    3. solarized  (warm, low contrast)");
    println!("    4. monokai    (green accent, code-friendly)");
    println!("    5. nord       (cool blues, arctic)");
    println!();

    let theme = if let Some(ref existing) = config.theme {
        let input = prompt(&format!("  Theme [{}]: ", existing))?;
        if input.is_empty() { existing.clone() } else { resolve_theme(&input) }
    } else {
        let input = prompt("  Select [1-5] [default: 1]: ")?;
        resolve_theme(&input)
    };

    // ─── Step 6: Project Setup ──────────────────────────
    println!();
    println!("  Step 6/7: Project setup");

    let cwd = std::env::current_dir().unwrap_or_default();
    let is_git = cwd.join(".git").exists();

    if is_git {
        // Offer to update .gitignore
        let gitignore_path = cwd.join(".gitignore");
        let needs_update = if gitignore_path.exists() {
            let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
            !content.contains(".oxshell") || !content.contains("*.mmdb")
        } else {
            true
        };

        if needs_update {
            println!();
            let input = prompt("  Add oxshell entries to .gitignore? [Y/n]: ")?;
            if input.to_lowercase() != "n" {
                update_gitignore(&gitignore_path)?;
                println!("  Updated .gitignore");
            }
        } else {
            println!("  .gitignore already has oxshell entries");
        }
    }

    // Offer to create CLAUDE.md
    let claude_md = cwd.join("CLAUDE.md");
    if !claude_md.exists() {
        println!();
        let input = prompt("  Create CLAUDE.md (project memory file)? [Y/n]: ")?;
        if input.to_lowercase() != "n" {
            create_claude_md(&claude_md, &cwd)?;
            println!("  Created CLAUDE.md");
        }
    } else {
        println!("  CLAUDE.md already exists");
    }

    // ─── Save Config ────────────────────────────────────
    config.cf_token = Some(token);
    config.account_id = Some(account_id);
    config.model = Some(model.clone());
    config.theme = Some(theme);
    config.save()?;

    println!();
    println!("  Config saved to: {}", OxshellConfig::path().display());

    // ─── Step 7: Getting Started Tutorial ───────────────
    println!();
    println!("  Step 7/7: Getting started");
    println!();
    println!("  ┌─────────────────────────────────────────────────┐");
    println!("  │  You're all set! Here's what to try:            │");
    println!("  │                                                 │");
    println!("  │  1. Quick test:                                 │");
    println!("  │     oxshell -p \"Hello, what can you do?\"        │");
    println!("  │                                                 │");
    println!("  │  2. Interactive TUI:                            │");
    println!("  │     oxshell                                     │");
    println!("  │                                                 │");
    println!("  │  3. Code review:                                │");
    println!("  │     oxshell -p \"/review\"                        │");
    println!("  │                                                 │");
    println!("  │  4. Multi-agent:                                │");
    println!("  │     oxshell --coordinator -p \"Analyze project\"  │");
    println!("  │                                                 │");
    println!("  │  5. Check health:                               │");
    println!("  │     oxshell doctor                              │");
    println!("  │                                                 │");
    println!("  │  TUI shortcuts:                                 │");
    println!("  │     /help    /skills    /memory    /doctor      │");
    println!("  │     /commit  /review    /simplify               │");
    println!("  │                                                 │");
    println!("  │  Create custom skills:                          │");
    println!("  │     mkdir -p .oxshell/skills/my-skill           │");
    println!("  │     # edit .oxshell/skills/my-skill/SKILL.md    │");
    println!("  │                                                 │");
    println!("  │  Docs: https://github.com/MauricioPerera/oxshell│");
    println!("  └─────────────────────────────────────────────────┘");
    println!();

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────

async fn detect_accounts(token: &str) -> Result<Vec<(String, String)>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.cloudflare.com/client/v4/accounts")
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to connect to Cloudflare API")?;

    if !resp.status().is_success() {
        bail!("Invalid token or API error (HTTP {})", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    Ok(body
        .get("result")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some((
                        a.get("id")?.as_str()?.to_string(),
                        a.get("name")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default())
}

async fn test_connection(token: &str, account_id: &str) -> Result<usize> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/ai/models/search",
        account_id
    );
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Connection test failed")?;

    if !resp.status().is_success() {
        bail!("API returned HTTP {}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    Ok(body.get("result").and_then(|r| r.as_array()).map(|a| a.len()).unwrap_or(0))
}

fn resolve_model_shorthand(input: &str) -> String {
    match input.trim() {
        "" | "1" => "@hf/nousresearch/hermes-2-pro-mistral-7b".to_string(),
        "2" => "@cf/ibm-granite/granite-4.0-h-micro".to_string(),
        "3" => "@cf/meta/llama-4-scout-17b-16e-instruct".to_string(),
        other => other.to_string(),
    }
}

fn resolve_theme(input: &str) -> String {
    match input.trim() {
        "" | "1" => "dark".to_string(),
        "2" => "light".to_string(),
        "3" => "solarized".to_string(),
        "4" => "monokai".to_string(),
        "5" => "nord".to_string(),
        other => other.to_string(),
    }
}

fn mask_token(token: &str) -> String {
    if token.len() > 8 {
        format!("{}...{}", &token[..4], &token[token.len() - 4..])
    } else {
        "****".to_string()
    }
}

fn prompt(message: &str) -> Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn update_gitignore(path: &Path) -> Result<()> {
    let mut content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };

    let entries = [
        "\n# oxshell",
        ".oxshell/mcp.json",
        ".oxshell/MEMORY.md",
        "*.mmdb",
        "*.mmdb.bak",
    ];

    for entry in &entries {
        if !content.contains(entry.trim_start_matches('\n')) {
            content.push_str(entry);
            content.push('\n');
        }
    }

    std::fs::write(path, content)?;
    Ok(())
}

fn create_claude_md(path: &Path, cwd: &Path) -> Result<()> {
    let project_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");

    let content = format!(
        "# {project_name}\n\
         \n\
         ## About\n\
         \n\
         <!-- Describe your project here. oxshell reads this file to understand context. -->\n\
         \n\
         ## Tech Stack\n\
         \n\
         <!-- List your languages, frameworks, and tools -->\n\
         \n\
         ## Conventions\n\
         \n\
         <!-- Coding style, naming conventions, branch strategy, etc. -->\n\
         \n\
         ## Important Files\n\
         \n\
         <!-- Key files the AI should know about -->\n\
         \n\
         ## Notes\n\
         \n\
         <!-- Anything else the AI should remember across sessions -->\n"
    );

    std::fs::write(path, content)?;
    Ok(())
}
