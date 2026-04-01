use anyhow::{Context, Result, bail};
use std::io::{self, Write};

use super::OxshellConfig;

/// Run the interactive setup wizard
pub async fn run_setup() -> Result<()> {
    println!();
    println!("  oxshell setup");
    println!("  =============");
    println!();

    let mut config = OxshellConfig::load();

    // Step 1: Cloudflare API Token
    println!("  Step 1: Cloudflare API Token");
    println!();
    println!("  You need a Cloudflare API token with Workers AI permissions.");
    println!("  Create one at: https://dash.cloudflare.com/profile/api-tokens");
    println!();
    println!("  Click 'Create Token' > 'Custom token' with these permissions:");
    println!("    - Account > Workers AI > Read");
    println!();

    let token = if let Some(ref existing) = config.cf_token {
        let masked = mask_token(existing);
        let input = prompt(&format!("  API Token [{}]: ", masked))?;
        if input.is_empty() {
            existing.clone()
        } else {
            input
        }
    } else {
        let input = prompt("  API Token: ")?;
        if input.is_empty() {
            bail!("API token is required. Get one at https://dash.cloudflare.com/profile/api-tokens");
        }
        input
    };

    // Step 2: Detect Account ID automatically
    println!();
    println!("  Step 2: Cloudflare Account ID");
    println!();
    println!("  Detecting your accounts...");

    let accounts = detect_accounts(&token).await?;

    let account_id = if accounts.is_empty() {
        println!("  Could not auto-detect accounts.");
        println!("  Find your Account ID at: https://dash.cloudflare.com");
        println!("  (It's in the URL: dash.cloudflare.com/<account-id>/...)");
        let input = prompt("  Account ID: ")?;
        if input.is_empty() {
            bail!("Account ID is required");
        }
        input
    } else if accounts.len() == 1 {
        let (id, name) = &accounts[0];
        println!("  Found: {} ({})", name, id);
        let input = prompt(&format!("  Use this account? [Y/n]: "))?;
        if input.to_lowercase() == "n" {
            let custom = prompt("  Account ID: ")?;
            if custom.is_empty() { id.clone() } else { custom }
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
        if idx > 0 && idx <= accounts.len() {
            accounts[idx - 1].0.clone()
        } else {
            accounts[0].0.clone()
        }
    };

    // Step 3: Test the connection
    println!();
    println!("  Step 3: Testing connection...");

    match test_connection(&token, &account_id).await {
        Ok(model_count) => {
            println!("  Connected! {} AI models available.", model_count);
        }
        Err(e) => {
            println!("  Warning: Connection test failed: {e}");
            println!("  Config will be saved anyway — you can fix it later.");
        }
    }

    // Step 4: Model selection
    println!();
    println!("  Step 4: Default model");
    println!();
    println!("  Recommended models:");
    println!("    1. @hf/nousresearch/hermes-2-pro-mistral-7b  (fast, free tier, basic tool calling)");
    println!("    2. @cf/ibm-granite/granite-4.0-h-micro       (131K context, best tool calling, paid)");
    println!("    3. @cf/meta/llama-4-scout-17b-16e-instruct    (newest, 17B params, paid)");
    println!();

    let model = if let Some(ref existing) = config.model {
        let input = prompt(&format!("  Model [{}]: ", existing))?;
        if input.is_empty() { existing.clone() } else { resolve_model_shorthand(&input) }
    } else {
        let input = prompt("  Select [1-3] or model name [default: 1]: ")?;
        resolve_model_shorthand(&input)
    };

    // Save config
    config.cf_token = Some(token);
    config.account_id = Some(account_id);
    config.model = Some(model.clone());
    config.save()?;

    println!();
    println!("  Config saved to: {}", OxshellConfig::path().display());
    println!();
    println!("  You're ready! Try:");
    println!("    oxshell -p \"Hello, what can you do?\"");
    println!("    oxshell   (interactive TUI)");
    println!();

    Ok(())
}

/// Detect Cloudflare accounts using the API token
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
    let accounts = body
        .get("result")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let id = a.get("id")?.as_str()?.to_string();
                    let name = a.get("name")?.as_str()?.to_string();
                    Some((id, name))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(accounts)
}

/// Quick connection test — try to list models
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
    let count = body
        .get("result")
        .and_then(|r| r.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(count)
}

fn resolve_model_shorthand(input: &str) -> String {
    match input.trim() {
        "" | "1" => "@hf/nousresearch/hermes-2-pro-mistral-7b".to_string(),
        "2" => "@cf/ibm-granite/granite-4.0-h-micro".to_string(),
        "3" => "@cf/meta/llama-4-scout-17b-16e-instruct".to_string(),
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
