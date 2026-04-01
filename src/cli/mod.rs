use clap::{Parser, Subcommand};

/// oxshell — AI coding assistant for the terminal, powered by Cloudflare Workers AI
#[derive(Parser, Debug, Clone)]
#[command(name = "oxshell", version, about)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Prompt to run in non-interactive mode (pipe mode)
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Resume a previous session (optionally pass session ID or prefix)
    #[arg(long)]
    pub resume: Option<String>,

    /// Cloudflare API token (overrides config + env)
    #[arg(long)]
    pub cf_token: Option<String>,

    /// Cloudflare Account ID (overrides config + env)
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

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Interactive setup wizard
    Setup,
    /// List recent sessions
    Sessions {
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
    },
    /// Run diagnostic checks
    Doctor,
    /// Start HTTP + WebSocket bridge server
    Serve {
        #[arg(short, long, default_value_t = 3080)]
        port: u16,
    },
}
