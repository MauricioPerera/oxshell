mod cli;
mod config;
mod context;
mod llm;
mod mcp;
mod memory;
mod permissions;
mod skills;
mod storage;
mod tasks;
mod tools;
mod ui;

use anyhow::Result;
use clap::Parser;
use std::path::Path;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use crate::cli::Args;
use crate::context::Context;
use crate::llm::WorkersAIClient;
use crate::llm::types::Message;
use crate::memory::store::MemoryStore;
use crate::permissions::PermissionManager;
use crate::skills::SkillRegistry;
use crate::storage::ConversationStore;
use crate::tools::ToolRegistry;
use crate::ui::App;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Handle subcommands
    if let Some(cli::Command::Setup) = args.command {
        return config::setup::run_setup().await;
    }

    // Load config (CLI flags > env vars > ~/.oxshell/config.json)
    let cfg = config::OxshellConfig::load();
    let resolved_token = cfg.resolve_token(&args.cf_token);
    let resolved_account = cfg.resolve_account_id(&args.account_id);
    let resolved_model = cfg.resolve_model(&args.model);

    // Check if configured — prompt setup if not
    if resolved_token.is_none() || resolved_account.is_none() {
        eprintln!("oxshell is not configured. Run: oxshell setup");
        eprintln!();
        eprintln!("Or set environment variables:");
        eprintln!("  export CLOUDFLARE_API_TOKEN=\"your-token\"");
        eprintln!("  export CLOUDFLARE_ACCOUNT_ID=\"your-account-id\"");
        std::process::exit(1);
    }

    let cwd = Path::new(&args.cwd);

    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("oxshell");
    std::fs::create_dir_all(&data_dir)?;

    let conversations = ConversationStore::new(&data_dir)?;
    let memory = MemoryStore::new(&data_dir)?;

    // Discover skills
    let skill_registry = SkillRegistry::new(cwd);

    // Initialize tools + register SkillTool with available skill names
    let mut tools = ToolRegistry::new();
    let skill_names: Vec<&str> = skill_registry
        .model_invocable()
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    if !skill_names.is_empty() {
        tools.register_skill_tool(&skill_names);
    }

    // Connect MCP servers and register their tools
    let mcp_manager = mcp::MCPManager::init(cwd, &mut tools).await?;

    // Register A2E tool if server configured
    if let Some(a2e_tool) = crate::tools::a2e::A2ETool::from_env() {
        tools.register_external(Box::new(a2e_tool));
        tracing::info!("A2E tool registered");
    }

    let client = WorkersAIClient::new(
        resolved_token,
        resolved_account,
        resolved_model,
    )?;

    let permissions = PermissionManager::new(args.auto_approve);
    let context = Context::new(args.clone(), conversations, memory);

    // Create task manager (notification receiver goes to TUI or oneshot loop)
    let (task_manager, task_notification_rx) = crate::tasks::TaskManager::new();
    let task_manager = std::sync::Arc::new(tokio::sync::Mutex::new(task_manager));

    // Register coordinator tools if --coordinator flag
    if args.coordinator {
        let (cf_token, account_id, model) = client.credentials();
        let tool_schema = tools.schema();

        // Dynamic system prompt builder — captures context reference
        let args_clone = args.clone();
        let system_prompt_fn: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(move || {
            let mut prompt = "You are an oxshell worker agent. Execute the task described in the prompt using the available tools.".to_string();
            if args_clone.coordinator {
                prompt.push_str("\nYou are a worker spawned by a coordinator. Focus on your specific task.");
            }
            prompt
        });

        tools.register_external(Box::new(crate::tools::task_tools::SpawnAgentTool {
            task_manager: task_manager.clone(),
            cf_token,
            account_id,
            model,
            system_prompt_fn,
            tool_schema,
        }));
        tools.register_external(Box::new(crate::tools::task_tools::SpawnBashTool {
            task_manager: task_manager.clone(),
        }));
        tools.register_external(Box::new(crate::tools::task_tools::TaskListTool {
            task_manager: task_manager.clone(),
        }));
        tools.register_external(Box::new(crate::tools::task_tools::TaskStopTool {
            task_manager: task_manager.clone(),
        }));

        tracing::info!("Coordinator mode enabled");
    }

    tracing::info!(
        "oxshell started — {} memories, {} skills, {} MCP servers, coordinator: {}, model: {}",
        context.memory.count(),
        skill_registry.active_skills().len(),
        mcp_manager.server_count(),
        args.coordinator,
        client.model
    );

    // Startup maintenance
    if context.memory.needs_consolidation() {
        tracing::info!("Memory store over limit, consolidating...");
        let _ = context.memory.consolidate();
    }

    if let Some(ref prompt) = args.prompt {
        let result = run_oneshot(
            &client,
            &tools,
            &permissions,
            &context,
            &skill_registry,
            prompt,
            task_notification_rx,
        )
        .await;
        context.flush();
        task_manager.lock().await.shutdown().await;
        mcp_manager.shutdown().await;
        return result;
    }

    let mut app = App::new(client, tools, permissions, context, skill_registry, task_notification_rx)?;
    let result = app.run().await;
    task_manager.lock().await.shutdown().await;
    mcp_manager.shutdown().await;
    result
}

/// Run a single prompt and exit (pipe mode).
/// In coordinator mode, waits for task notifications between turns.
async fn run_oneshot(
    client: &WorkersAIClient,
    tools: &ToolRegistry,
    permissions: &PermissionManager,
    context: &Context,
    skills: &SkillRegistry,
    prompt: &str,
    mut task_rx: tokio::sync::mpsc::Receiver<crate::tasks::manager::TaskNotification>,
) -> Result<()> {
    let mut messages = vec![Message::user(prompt.to_string())];

    let mut system = context.build_system_prompt();

    // Add skills section to system prompt
    let skills_section = skills.prompt_section();
    if !skills_section.is_empty() {
        system.push_str("\n\n");
        system.push_str(&skills_section);
    }

    let relevant = context.build_relevant_memories(prompt);
    if !relevant.is_empty() {
        system.push_str("\n\n");
        system.push_str(&relevant);
    }

    const MAX_TURNS: usize = 15;

    for _turn in 0..MAX_TURNS {
        let response = client
            .send_message(&system, &messages, &tools.schema())
            .await?;

        let choice = match response.choices.first() {
            Some(c) => c,
            None => break,
        };

        let msg = match &choice.message {
            Some(m) => m,
            None => break,
        };

        if let Some(ref text) = msg.content {
            println!("{text}");
        }

        if let Some(ref tool_calls) = msg.tool_calls {
            let tool_desc: String = tool_calls
                .iter()
                .map(|tc| {
                    format!(
                        "[Calling tool: {} with args: {}]",
                        tc.function.name, tc.function.arguments
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            messages.push(Message::assistant_text(tool_desc));

            let mut results = Vec::new();
            for tc in tool_calls {
                // Intercept skill tool calls
                if tc.function.name == "skill" {
                    let input = tc.function.parse_arguments();
                    let skill_name = input
                        .get("skill")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let skill_args = input
                        .get("args")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    eprintln!("[skill: {skill_name}]");

                    if let Some(skill) = skills.get(skill_name) {
                        match crate::skills::execution::execute_skill(
                            skill,
                            skill_args,
                            client,
                            tools,
                            permissions,
                            &system,
                        )
                        .await
                        {
                            Ok(crate::skills::execution::SkillResult::Inline(prompt)) => {
                                results.push(format!(
                                    "[Skill '{}' prompt]:\n{}",
                                    skill_name, prompt
                                ));
                            }
                            Ok(crate::skills::execution::SkillResult::Forked(output)) => {
                                results.push(format!(
                                    "[Skill '{}' result]:\n{}",
                                    skill_name, output
                                ));
                            }
                            Err(e) => {
                                results.push(format!(
                                    "[Skill '{}' ERROR]: {}",
                                    skill_name, e
                                ));
                            }
                        }
                    } else {
                        results.push(format!("[Skill '{skill_name}' not found]"));
                    }
                    continue;
                }

                eprintln!("[tool: {}]", tc.function.name);
                let input = tc.function.parse_arguments();

                let output = tools.execute(&tc.function.name, &input, permissions).await?;
                if output.is_error {
                    results.push(format!(
                        "[Tool '{}' ERROR]:\n{}",
                        tc.function.name, output.content
                    ));
                } else {
                    results.push(format!(
                        "[Tool '{}' returned]:\n{}",
                        tc.function.name, output.content
                    ));
                }
            }

            messages.push(Message::user(results.join("\n\n")));

            // In coordinator mode: wait for any task notifications before next turn
            if context.args.coordinator {
                // Give tasks a moment to complete, then drain all notifications
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                while let Ok(notif) = task_rx.try_recv() {
                    eprintln!("[task completed: {}]", notif.task_id);
                    messages.push(Message::user(notif.xml));
                }
            }

            continue;
        }

        // No tool calls — but in coordinator mode, wait for pending task notifications
        if context.args.coordinator {
            let has_running = !messages.iter().any(|m| {
                m.content.as_deref().unwrap_or("").contains("<task-notification>")
            });
            if has_running {
                // Wait up to 30s for task notifications
                match tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    task_rx.recv(),
                ).await {
                    Ok(Some(notif)) => {
                        eprintln!("[task completed: {}]", notif.task_id);
                        messages.push(Message::user(notif.xml));
                        // Drain any additional notifications
                        while let Ok(n) = task_rx.try_recv() {
                            eprintln!("[task completed: {}]", n.task_id);
                            messages.push(Message::user(n.xml));
                        }
                        continue; // Send notifications to model
                    }
                    _ => {} // Timeout or no tasks — exit
                }
            }
        }

        break;
    }

    // Post-conversation: extract memories
    let mut extractor =
        crate::memory::extraction::MemoryExtractor::new(&context.memory, &context.session_id);
    let extracted = extractor.extract_from_messages(&messages)?;
    if extracted > 0 {
        eprintln!("[memory: {extracted} entries extracted]");
    }

    Ok(())
}
