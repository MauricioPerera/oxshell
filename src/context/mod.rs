use std::path::Path;

use crate::cli::Args;
use crate::memory::index::MemoryIndex;
use crate::memory::retrieval::MemoryRetriever;
use crate::memory::store::MemoryStore;
use crate::storage::ConversationStore;

/// Manages session context: system prompt, memory, working directory
pub struct Context {
    pub args: Args,
    pub conversations: ConversationStore,
    pub memory: MemoryStore,
    pub memory_index: MemoryIndex,
    pub session_id: String,
    pub cwd: String,
}

impl Context {
    pub fn new(
        args: Args,
        conversations: ConversationStore,
        memory: MemoryStore,
    ) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        let cwd = args.cwd.clone();
        let memory_index = MemoryIndex::new(Path::new(&cwd));

        // Bootstrap: index CLAUDE.md into memory store if not already present
        if let Err(e) = memory.bootstrap_from_claude_md(Path::new(&cwd), &session_id) {
            tracing::warn!("Failed to bootstrap CLAUDE.md: {e}");
        }

        // Rebuild MEMORY.md index
        if let Err(e) = memory_index.rebuild(&memory) {
            tracing::warn!("Failed to rebuild MEMORY.md: {e}");
        }

        Self {
            args,
            conversations,
            memory,
            memory_index,
            session_id,
            cwd,
        }
    }

    /// Build the system prompt with all available context.
    /// Includes: base identity + custom prompt + env + MEMORY.md index + relevant memories
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        // Base identity
        parts.push(
            "You are oxshell, an AI coding assistant running in the user's terminal, \
             powered by Cloudflare Workers AI. You help with coding tasks by reading files, \
             writing code, running commands, and searching codebases.\n\
             \n\
             You have access to tools for filesystem operations and command execution. \
             Be concise and direct. Prefer editing existing files over creating new ones. \
             Always read files before editing them."
                .to_string(),
        );

        // Coordinator mode injection
        if self.args.coordinator {
            parts.push(crate::tasks::coordinator::coordinator_system_prompt());
        }

        // Custom system prompt
        if let Some(ref custom) = self.args.system_prompt {
            parts.push(custom.clone());
        }

        // Environment
        parts.push(format!("Working directory: {}", self.cwd));
        parts.push(format!(
            "Current date: {}",
            chrono::Local::now().format("%Y-%m-%d")
        ));

        // MEMORY.md index (up to 200 lines, like KAIROS)
        if let Some(index) = self.memory_index.load() {
            if !index.is_empty() {
                parts.push(index);
            }
        }

        parts.join("\n\n")
    }

    /// Build relevant memories section for a specific query.
    /// Uses hybrid BM25 + vector search (zero API calls).
    pub fn build_relevant_memories(&self, query: &str) -> String {
        let mut retriever = MemoryRetriever::new(&self.memory);
        match retriever.format_for_prompt(query) {
            Ok(text) if !text.is_empty() => text,
            _ => String::new(),
        }
    }

    /// Flush all stores to disk
    pub fn flush(&self) {
        let _ = self.conversations.flush();
        let _ = self.memory.flush();
        let _ = self.memory_index.rebuild(&self.memory);
    }
}
