use clap::Parser;

/// oxshell — AI coding assistant for the terminal, powered by Cloudflare Workers AI
#[derive(Parser, Debug, Clone)]
#[command(name = "oxshell", version, about)]
pub struct Args {
    /// Prompt to run in non-interactive mode (pipe mode)
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Cloudflare API token (defaults to CLOUDFLARE_API_TOKEN env var)
    #[arg(long)]
    pub cf_token: Option<String>,

    /// Cloudflare Account ID (defaults to CLOUDFLARE_ACCOUNT_ID env var)
    #[arg(long)]
    pub account_id: Option<String>,

    /// Workers AI model to use
    #[arg(short, long, default_value = "@hf/nousresearch/hermes-2-pro-mistral-7b")]
    pub model: String,

    /// Working directory
    #[arg(short = 'd', long, default_value = ".")]
    pub cwd: String,

    /// Auto-approve all tool executions (dangerous)
    #[arg(long, default_value_t = false)]
    pub auto_approve: bool,

    /// Enable coordinator mode (multi-agent orchestration)
    #[arg(long, default_value_t = false)]
    pub coordinator: bool,

    /// Max tokens for responses
    #[arg(long, default_value_t = 1024)]
    pub max_tokens: u32,

    /// System prompt override
    #[arg(long)]
    pub system_prompt: Option<String>,

    /// Verbose logging
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}
