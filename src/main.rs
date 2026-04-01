mod a2e;
mod cli;
mod compaction;
mod config;
mod context;
mod cost;
mod doctor;
mod hooks;
mod llm;
mod mcp;
mod memory;
mod permissions;
mod plugins;
mod session;
mod skills;
mod storage;
mod tasks;
mod tools;
mod ui;
mod vim;

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
use crate::session::SessionStore;
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
    match &args.command {
        Some(cli::Command::Setup) => return config::setup::run_setup().await,
        Some(cli::Command::Sessions { limit }) => {
            let data_dir = dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("oxshell");
            let session_store = SessionStore::new(&data_dir)?;
            let sessions = session_store.recent(*limit)?;
            if sessions.is_empty() {
                println!("No sessions yet.");
            } else {
                println!("{:<10} {:<20} {}", "ID", "Date", "Title");
                println!("{}", "-".repeat(60));
                for meta in sessions {
                    println!(
                        "{:<10} {:<20} {}",
                        &meta.id[..meta.id.len().min(9)],
                        meta.updated_at.format("%Y-%m-%d %H:%M"),
                        meta.title
                    );
                }
            }
            return Ok(());
        }
        Some(cli::Command::Doctor) => {
            let cfg = config::OxshellConfig::load();
            let cwd = Path::new(&args.cwd);
            let data_dir = dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("oxshell");
            let memory = crate::memory::store::MemoryStore::new(&data_dir).ok();
            let mem_count = memory.as_ref().map(|m| m.count()).unwrap_or(0);
            let plugin_registry = plugins::PluginRegistry::new(cwd);
            let checks = doctor::run_diagnostics(cwd, &cfg, &plugin_registry, mem_count);
            println!("{}", doctor::format_diagnostics(&checks));
            return Ok(());
        }
        None => {}
    }

    // Load config
    let cfg = config::OxshellConfig::load();
    let resolved_token = cfg.resolve_token(&args.cf_token);
    let resolved_account = cfg.resolve_account_id(&args.account_id);
    let resolved_model = cfg.resolve_model(&args.model);

    if resolved_token.is_none() || resolved_account.is_none() {
        eprintln!("oxshell is not configured. Run: oxshell setup");
        std::process::exit(1);
    }

    let cwd = Path::new(&args.cwd);
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("oxshell");
    std::fs::create_dir_all(&data_dir)?;

    // Initialize stores
    let conversations = ConversationStore::new(&data_dir)?;
    let memory = MemoryStore::new(&data_dir)?;
    let session_store = SessionStore::new(&data_dir)?;

    // Handle --resume: load previous session or generate new ID
    let (session_id, initial_messages) = if let Some(ref resume_query) = args.resume {
        let query = if resume_query.is_empty() {
            // --resume with no value → most recent session
            session_store
                .recent(1)?
                .first()
                .map(|m| m.id.clone())
                .unwrap_or_default()
        } else {
            resume_query.clone()
        };

        if let Some(meta) = session_store.find_session(&query) {
            let messages = session_store.load_messages(&meta.id)?;
            eprintln!("[resumed session: {} — {} messages]", &meta.id[..8], messages.len());
            (meta.id, messages)
        } else {
            eprintln!("Session not found: {query}");
            eprintln!("Run 'oxshell sessions' to list available sessions.");
            std::process::exit(1);
        }
    } else {
        (uuid::Uuid::new_v4().to_string(), Vec::new())
    };

    // Build context with session store
    let context = Context::new(args.clone(), conversations, memory, session_store, session_id);

    // Discover skills
    let skill_registry = SkillRegistry::new(cwd);

    // Initialize tools
    let mut tools = ToolRegistry::new();
    let skill_names: Vec<&str> = skill_registry
        .model_invocable()
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    if !skill_names.is_empty() {
        tools.register_skill_tool(&skill_names);
    }

    let mcp_manager = mcp::MCPManager::init(cwd, &mut tools).await?;
    tools.register_external(Box::new(crate::tools::a2e::A2ETool));

    let client = WorkersAIClient::new(resolved_token, resolved_account, resolved_model)?;
    let permissions = PermissionManager::new(args.auto_approve);

    // Task manager
    let (task_manager, task_notification_rx) = crate::tasks::TaskManager::new();
    let task_manager = std::sync::Arc::new(tokio::sync::Mutex::new(task_manager));

    if args.coordinator {
        let (cf_token, account_id, model) = client.credentials();
        let tool_schema = tools.schema();
        let args_clone = args.clone();
        let system_prompt_fn: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(move || {
            let mut prompt = "You are an oxshell worker agent.".to_string();
            if args_clone.coordinator {
                prompt.push_str(" Focus on your specific task.");
            }
            prompt
        });

        tools.register_external(Box::new(crate::tools::task_tools::SpawnAgentTool {
            task_manager: task_manager.clone(), cf_token, account_id, model,
            system_prompt_fn, tool_schema,
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
    }

    tracing::info!(
        "oxshell — {} memories, {} skills, session: {}, model: {}",
        context.memory.count(),
        skill_registry.active_skills().len(),
        &context.session_id[..8],
        client.model
    );

    if context.memory.needs_consolidation() {
        let _ = context.memory.consolidate();
    }

    if let Some(ref prompt) = args.prompt {
        let result = run_oneshot(
            &client, &tools, &permissions, &context, &skill_registry,
            prompt, task_notification_rx, &initial_messages,
        ).await;
        context.flush();
        task_manager.lock().await.shutdown().await;
        mcp_manager.shutdown().await;
        return result;
    }

    let mut app = App::new(
        client, tools, permissions, context, skill_registry,
        task_notification_rx, initial_messages,
    )?;
    let result = app.run().await;
    task_manager.lock().await.shutdown().await;
    mcp_manager.shutdown().await;
    result
}

/// Run a single prompt and exit (pipe mode)
async fn run_oneshot(
    client: &WorkersAIClient,
    tools: &ToolRegistry,
    permissions: &PermissionManager,
    context: &Context,
    skills: &SkillRegistry,
    prompt: &str,
    mut task_rx: tokio::sync::mpsc::Receiver<crate::tasks::manager::TaskNotification>,
    initial_messages: &[Message],
) -> Result<()> {
    let mut messages: Vec<Message> = initial_messages.to_vec();
    messages.push(Message::user(prompt.to_string()));
    context.persist_message(messages.last().unwrap());

    let mut system = context.build_system_prompt();
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

    for turn in 0..MAX_TURNS {
        if turn == MAX_TURNS - 1 {
            eprintln!("[warning: max turns ({MAX_TURNS}) reached]");
        }

        // Auto-compact if approaching context limit
        if let Ok(Some(result)) = crate::compaction::maybe_compact(client, &messages, &system).await {
            eprintln!("[compacted: {} -> {} messages]", result.original_count, result.compacted_count);
            messages = result.compacted_messages;
        }

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
            let tool_desc: String = tool_calls.iter()
                .map(|tc| format!("[Calling: {} ({})]", tc.function.name, tc.function.arguments))
                .collect::<Vec<_>>().join("\n");
            let assistant_msg = Message::assistant_text(tool_desc);
            context.persist_message(&assistant_msg);
            messages.push(assistant_msg);

            let mut results = Vec::new();
            for tc in tool_calls {
                if tc.function.name == "skill" {
                    let input = tc.function.parse_arguments();
                    let sn = input.get("skill").and_then(|v| v.as_str()).unwrap_or("");
                    let sa = input.get("args").and_then(|v| v.as_str()).unwrap_or("");
                    eprintln!("[skill: {sn}]");
                    if let Some(skill) = skills.get(sn) {
                        match crate::skills::execution::execute_skill(skill, sa, client, tools, permissions, &system).await {
                            Ok(crate::skills::execution::SkillResult::Inline(p)) => results.push(format!("[Skill '{sn}']:\n{p}")),
                            Ok(crate::skills::execution::SkillResult::Forked(o)) => results.push(format!("[Skill '{sn}']:\n{o}")),
                            Err(e) => results.push(format!("[Skill '{sn}' ERROR]: {e}")),
                        }
                    } else { results.push(format!("[Skill '{sn}' not found]")); }
                    continue;
                }

                eprintln!("[tool: {}]", tc.function.name);
                let input = tc.function.parse_arguments();
                let output = tools.execute(&tc.function.name, &input, permissions).await?;
                let prefix = if output.is_error { "ERROR" } else { "returned" };
                results.push(format!("[Tool '{}' {prefix}]:\n{}", tc.function.name, output.content));
            }

            let result_msg = Message::user(results.join("\n\n"));
            context.persist_message(&result_msg);
            messages.push(result_msg);

            if context.args.coordinator {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                while let Ok(notif) = task_rx.try_recv() {
                    eprintln!("[task completed: {}]", notif.task_id);
                    let n = Message::user(notif.xml);
                    context.persist_message(&n);
                    messages.push(n);
                }
            }
            continue;
        }

        // Persist assistant response
        if let Some(ref text) = msg.content {
            context.persist_message(&Message::assistant_text(text.clone()));
        }

        // Coordinator: wait for pending task notifications (max 3 retries, 30s each)
        if context.args.coordinator {
            let mut retries = 0;
            const MAX_NOTIFICATION_RETRIES: usize = 3;
            while retries < MAX_NOTIFICATION_RETRIES {
                match tokio::time::timeout(std::time::Duration::from_secs(30), task_rx.recv()).await {
                    Ok(Some(notif)) => {
                        eprintln!("[task completed: {}]", notif.task_id);
                        let n = Message::user(notif.xml);
                        context.persist_message(&n);
                        messages.push(n);
                        while let Ok(n2) = task_rx.try_recv() {
                            let m = Message::user(n2.xml);
                            context.persist_message(&m);
                            messages.push(m);
                        }
                        break; // Got notifications, continue conversation
                    }
                    _ => {
                        retries += 1;
                        if retries >= MAX_NOTIFICATION_RETRIES {
                            eprintln!("[coordinator: no task notifications after {MAX_NOTIFICATION_RETRIES} retries]");
                        }
                    }
                }
            }
            if retries < MAX_NOTIFICATION_RETRIES {
                continue; // Process received notifications
            }
        }
        break;
    }

    // Post-conversation: extract memories
    let mut extractor = crate::memory::extraction::MemoryExtractor::new(&context.memory, &context.session_id);
    let extracted = extractor.extract_from_messages(&messages)?;
    if extracted > 0 { eprintln!("[memory: {extracted} entries extracted]"); }

    Ok(())
}
