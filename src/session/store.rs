use anyhow::{Context, Result};
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::types::*;
use crate::llm::types::Message;

/// Persistent session storage using JSONL files.
/// Layout: ~/.oxshell/sessions/index.json + {date}/{session_id}.jsonl
pub struct SessionStore {
    sessions_dir: PathBuf,
}

impl SessionStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let sessions_dir = data_dir.join("sessions");
        fs::create_dir_all(&sessions_dir)?;
        Ok(Self { sessions_dir })
    }

    // ─── Write ──────────────────────────────────────────

    /// Append a message to a session file. Auto-creates index entry on first message.
    pub fn append(
        &self,
        session_id: &str,
        message: &Message,
        model: &str,
        cwd: &str,
    ) -> Result<()> {
        let entry = SessionEntry {
            timestamp: Utc::now(),
            message: message.clone(),
            is_compaction_summary: message
                .content
                .as_deref()
                .map(|c| c.starts_with("[Session context"))
                .unwrap_or(false),
        };

        // Ensure date directory exists
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let date_dir = self.sessions_dir.join(&date);
        fs::create_dir_all(&date_dir)?;

        // Append to JSONL file
        let file_path = date_dir.join(format!("{session_id}.jsonl"));
        let line = serde_json::to_string(&entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        writeln!(file, "{line}")?;

        // Upsert index entry
        let mut index = self.load_index().unwrap_or_default();
        if let Some(meta) = index.iter_mut().find(|m| m.id == session_id) {
            meta.updated_at = Utc::now();
            meta.message_count += 1;
        } else {
            // Auto-title from first user message
            let title = message
                .content
                .as_deref()
                .unwrap_or("(no title)")
                .chars()
                .take(80)
                .collect::<String>();

            index.push(SessionMeta {
                id: session_id.to_string(),
                title,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                message_count: 1,
                model: model.to_string(),
                cwd: cwd.to_string(),
            });
        }

        // Sort by most recent first and save
        index.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        self.save_index(&index)?;

        Ok(())
    }

    /// Rewrite an entire session file (used after compaction)
    #[allow(dead_code)]
    pub fn rewrite(&self, session_id: &str, entries: &[SessionEntry]) -> Result<()> {
        if let Some(path) = self.find_session_path(session_id) {
            let temp = path.with_extension("jsonl.tmp");
            let mut file = fs::File::create(&temp)?;
            for entry in entries {
                let line = serde_json::to_string(entry)?;
                writeln!(file, "{line}")?;
            }
            fs::rename(&temp, &path)?;
        }
        Ok(())
    }

    // ─── Read ───────────────────────────────────────────

    /// Load all messages from a session file
    pub fn load_session(&self, session_id: &str) -> Result<Vec<SessionEntry>> {
        let path = self
            .find_session_path(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{session_id}' not found"))?;

        let content = fs::read_to_string(&path)?;
        let mut entries = Vec::new();

        for (num, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<SessionEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!("Session {session_id} line {}: parse error: {e}", num + 1);
                }
            }
        }

        Ok(entries)
    }

    /// Load messages as Vec<Message> (for resume)
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let entries = self.load_session(session_id)?;
        Ok(entries.into_iter().map(|e| e.message).collect())
    }

    /// List recent sessions
    pub fn recent(&self, limit: usize) -> Result<Vec<SessionMeta>> {
        let index = self.load_index().unwrap_or_default();
        Ok(index.into_iter().take(limit).collect())
    }

    /// Find a session by ID or prefix
    pub fn find_session(&self, query: &str) -> Option<SessionMeta> {
        let index = self.load_index().ok()?;
        // Exact match first
        if let Some(meta) = index.iter().find(|m| m.id == query) {
            return Some(meta.clone());
        }
        // Prefix match
        index
            .into_iter()
            .find(|m| m.id.starts_with(query))
    }

    // ─── Index ──────────────────────────────────────────

    fn load_index(&self) -> Result<Vec<SessionMeta>> {
        let path = self.sessions_dir.join("index.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        let index: Vec<SessionMeta> = serde_json::from_str(&content)
            .context("Failed to parse session index")?;
        Ok(index)
    }

    fn save_index(&self, index: &[SessionMeta]) -> Result<()> {
        let path = self.sessions_dir.join("index.json");
        let json = serde_json::to_string_pretty(index)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Find the JSONL file for a session (scans date directories)
    fn find_session_path(&self, session_id: &str) -> Option<PathBuf> {
        let filename = format!("{session_id}.jsonl");

        // Scan date directories (newest first)
        let mut dirs: Vec<_> = fs::read_dir(&self.sessions_dir)
            .ok()?
            .flatten()
            .filter(|e| e.path().is_dir())
            .collect();
        dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        for dir in dirs {
            let candidate = dir.path().join(&filename);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        // Also check prefix match
        for dir in fs::read_dir(&self.sessions_dir).ok()?.flatten() {
            if !dir.path().is_dir() {
                continue;
            }
            for file in fs::read_dir(dir.path()).ok()?.flatten() {
                let name = file.file_name().to_string_lossy().to_string();
                if name.starts_with(session_id) && name.ends_with(".jsonl") {
                    return Some(file.path());
                }
            }
        }

        None
    }
}
