use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::parser::{parse_skill_content, parse_skill_file};
use super::types::*;

/// Registry of all discovered skills.
/// Supports filesystem scanning, bundled skills, and conditional activation.
pub struct SkillRegistry {
    /// All loaded skills, keyed by name
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    /// Create a new registry, scanning all skill directories
    pub fn new(cwd: &Path) -> Self {
        let mut registry = Self {
            skills: HashMap::new(),
        };

        // Load bundled skills first (lowest priority)
        registry.load_bundled_skills();

        // Scan filesystem skill directories (higher priority)
        let dirs = Self::skill_directories(cwd);
        for dir in dirs {
            if let Err(e) = registry.scan_directory(&dir) {
                tracing::warn!("Failed to scan skills directory {}: {e}", dir.display());
            }
        }

        let active = registry.skills.values().filter(|s| s.active).count();
        let conditional = registry.skills.values().filter(|s| !s.active).count();
        if active > 0 || conditional > 0 {
            tracing::info!(
                "Skills loaded: {active} active, {conditional} conditional, {} total",
                registry.skills.len()
            );
        }

        registry
    }

    /// Get all skill directories to scan (ordered by priority, lowest first)
    fn skill_directories(cwd: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // User-global: ~/.oxshell/skills/
        if let Some(home) = dirs::home_dir() {
            let user_dir = home.join(".oxshell").join("skills");
            if user_dir.exists() {
                dirs.push(user_dir);
            }
        }

        // Project: .oxshell/skills/ (walk up from cwd to home)
        let mut current = cwd.to_path_buf();
        let home = dirs::home_dir().unwrap_or_default();
        loop {
            let skill_dir = current.join(".oxshell").join("skills");
            if skill_dir.exists() {
                dirs.push(skill_dir);
            }
            // Also check .claude/skills/ for compatibility
            let claude_dir = current.join(".claude").join("skills");
            if claude_dir.exists() {
                dirs.push(claude_dir);
            }
            if current == home || !current.pop() {
                break;
            }
        }

        dirs
    }

    /// Scan a directory for skill subdirectories (each with SKILL.md)
    fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        let entries = std::fs::read_dir(dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            match parse_skill_file(&skill_file) {
                Ok(skill) => {
                    tracing::debug!(
                        "Loaded skill '{}' from {}",
                        skill.name,
                        skill_file.display()
                    );
                    self.skills.insert(skill.name.clone(), skill);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse skill at {}: {e}",
                        skill_file.display()
                    );
                }
            }
        }

        Ok(())
    }

    /// Register bundled skills (compiled into the binary)
    fn load_bundled_skills(&mut self) {
        // Built-in: simplify — review code for quality
        self.register_bundled(
            "simplify",
            "Review changed code for reuse, quality, and efficiency, then fix issues found.",
            "When the user asks to review, simplify, or clean up code",
            "Review the code I just changed. Look for:\n\
             1. Duplicated logic that could be extracted\n\
             2. Unnecessary complexity\n\
             3. Performance issues\n\
             4. Missing error handling\n\
             Then fix any issues you find. Be concise about what you changed.",
        );

        // Built-in: commit — AI-powered git commit
        self.register_bundled(
            "commit",
            "Create a git commit with an AI-generated message based on staged changes.",
            "When the user asks to commit changes",
            "Create a git commit for the current changes:\n\
             1. Run `git status` and `git diff --cached` to see staged changes\n\
             2. If nothing is staged, run `git add -A` first\n\
             3. Analyze the changes and write a concise commit message\n\
             4. The message should focus on WHY, not WHAT\n\
             5. Run `git commit -m \"<message>\"` to create the commit\n\
             6. Show the commit hash and summary",
        );

        // Built-in: review — code review
        self.register_bundled(
            "review",
            "Review code changes and provide feedback on quality, bugs, and improvements.",
            "When the user asks for a code review",
            "Review the recent code changes:\n\
             1. Run `git diff` to see unstaged changes, or `git diff HEAD~1` for last commit\n\
             2. Analyze for: bugs, security issues, performance, readability\n\
             3. Provide specific, actionable feedback\n\
             4. Be concise — highlight only important issues",
        );
    }

    fn register_bundled(&mut self, name: &str, desc: &str, when: &str, prompt: &str) {
        if let Ok(skill) = parse_skill_content(
            &format!(
                "---\nname: {name}\ndescription: {desc}\nwhen_to_use: {when}\n---\n\n{prompt}"
            ),
            name,
            Path::new("."),
            SkillSource::Bundled,
        ) {
            self.skills.insert(name.to_string(), skill);
        }
    }

    // ─── Public API ─────────────────────────────────────

    /// Get a skill by name (only if active)
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name).filter(|s| s.active)
    }

    /// Get all active skills
    pub fn active_skills(&self) -> Vec<&Skill> {
        self.skills.values().filter(|s| s.active).collect()
    }

    /// Get all user-invocable skills (for /skills command)
    pub fn user_invocable(&self) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| s.active && s.user_invocable)
            .collect()
    }

    /// Get all model-invocable skills (for SkillTool schema)
    pub fn model_invocable(&self) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| s.active && s.model_invocable)
            .collect()
    }

    /// Activate conditional skills that match the given file paths.
    /// Called when file tools (read/write/edit) are used.
    pub fn activate_for_paths(&mut self, file_paths: &[&str]) {
        let mut activated = Vec::new();

        for skill in self.skills.values_mut() {
            if skill.active || skill.paths.is_empty() {
                continue;
            }

            for file_path in file_paths {
                if skill.matches_path(file_path) {
                    skill.active = true;
                    activated.push(skill.name.clone());
                    break;
                }
            }
        }

        if !activated.is_empty() {
            tracing::info!("Activated conditional skills: {}", activated.join(", "));
        }
    }

    /// Generate description of available skills for system prompt
    pub fn prompt_section(&self) -> String {
        let model_skills = self.model_invocable();
        if model_skills.is_empty() {
            return String::new();
        }

        let mut lines = vec!["# Available Skills".to_string()];
        lines.push(String::new());
        lines.push(
            "You can invoke these skills using the `skill` tool:".to_string(),
        );

        for skill in &model_skills {
            lines.push(format!("- **{}**: {}", skill.name, skill.description));
            if !skill.when_to_use.is_empty() {
                lines.push(format!("  Use when: {}", skill.when_to_use));
            }
        }

        lines.join("\n")
    }
}
