use std::path::Path;

use crate::cli::Args;
use crate::memory::index::MemoryIndex;
use crate::memory::retrieval::MemoryRetriever;
use crate::memory::store::MemoryStore;
use crate::session::SessionStore;
use crate::storage::ConversationStore;

/// Manages session context: system prompt, memory, sessions, working directory
pub struct Context {
    pub args: Args,
    pub conversations: ConversationStore,
    pub memory: MemoryStore,
    pub memory_index: MemoryIndex,
    pub session: SessionStore,
    pub session_id: String,
    pub cwd: String,
}

impl Context {
    pub async fn new(
        args: Args,
        conversations: ConversationStore,
        memory: MemoryStore,
        session: SessionStore,
        session_id: String,
    ) -> Self {
        let cwd = args.cwd.clone();
        let memory_index = MemoryIndex::new(Path::new(&cwd));

        if let Err(e) = memory
            .bootstrap_from_claude_md(Path::new(&cwd), &session_id)
            .await
        {
            tracing::warn!("Failed to bootstrap CLAUDE.md: {e}");
        }

        if let Err(e) = memory_index.rebuild(&memory) {
            tracing::warn!("Failed to rebuild MEMORY.md: {e}");
        }

        Self {
            args,
            conversations,
            memory,
            memory_index,
            session,
            session_id,
            cwd,
        }
    }

    /// Build the system prompt with all available context
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();

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

        if self.args.coordinator {
            parts.push(crate::tasks::coordinator::coordinator_system_prompt());
        }

        if let Some(ref custom) = self.args.system_prompt {
            parts.push(custom.clone());
        }

        parts.push(format!("Working directory: {}", self.cwd));
        parts.push(format!(
            "Current date: {}",
            chrono::Local::now().format("%Y-%m-%d")
        ));

        if let Some(index) = self.memory_index.load() {
            if !index.is_empty() {
                parts.push(index);
            }
        }

        parts.join("\n\n")
    }

    /// Build relevant memories for a specific query (uses real embeddings)
    pub async fn build_relevant_memories(&self, query: &str) -> String {
        let retriever = MemoryRetriever::new(&self.memory);
        match retriever.format_for_prompt(query).await {
            Ok(text) if !text.is_empty() => text,
            _ => String::new(),
        }
    }

    /// Persist a message to the session file
    pub fn persist_message(&self, message: &crate::llm::types::Message) {
        let _ = self.session.append(
            &self.session_id,
            message,
            &self.args.model,
            &self.cwd,
        );
    }

    /// Flush all stores to disk
    pub fn flush(&self) {
        let _ = self.conversations.flush();
        let _ = self.memory.flush();
        let _ = self.memory_index.rebuild(&self.memory);
    }
}
