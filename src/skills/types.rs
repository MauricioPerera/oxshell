use std::path::PathBuf;

/// A loaded skill definition
#[derive(Debug, Clone)]
pub struct Skill {
    /// Unique name (derived from directory name)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// When should this skill be used (hint for the model)
    pub when_to_use: String,
    /// The full prompt content (markdown body after frontmatter)
    pub prompt: String,
    /// Argument placeholders ($1, $2, $ARGUMENTS)
    pub arguments: Vec<String>,
    /// Tools allowed during this skill's execution
    pub allowed_tools: Vec<String>,
    /// Execution context
    pub context: SkillContext,
    /// Can users invoke via /name?
    pub user_invocable: bool,
    /// Can model invoke via SkillTool?
    pub model_invocable: bool,
    /// File patterns for conditional activation (gitignore-style)
    pub paths: Vec<String>,
    /// Where this skill was loaded from
    pub source: SkillSource,
    /// Directory containing the SKILL.md file
    pub skill_dir: PathBuf,
    /// Whether this skill is currently active (conditional skills start inactive)
    pub active: bool,
}

/// How the skill should execute
#[derive(Debug, Clone, PartialEq)]
pub enum SkillContext {
    /// Inject prompt directly into conversation (fast, shared context)
    Inline,
    /// Run in isolated sub-agent with own token budget
    Fork,
}

/// Where the skill was loaded from
#[derive(Debug, Clone, PartialEq)]
pub enum SkillSource {
    /// Built into the binary
    Bundled,
    /// Loaded from .oxshell/skills/ directory
    Filesystem,
}

impl Skill {
    /// Substitute arguments into the prompt template
    pub fn render(&self, args: &str) -> String {
        let mut rendered = self.prompt.clone();

        // Replace $ARGUMENTS with full args string
        rendered = rendered.replace("$ARGUMENTS", args);
        rendered = rendered.replace("${ARGUMENTS}", args);

        // Replace $1, $2, etc. with positional args
        let parts: Vec<&str> = args.split_whitespace().collect();
        for (i, part) in parts.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            let placeholder_braced = format!("${{{}}}", i + 1);
            rendered = rendered.replace(&placeholder_braced, part);
            rendered = rendered.replace(&placeholder, part);
        }

        // Replace ${SKILL_DIR} with the skill's directory path
        let dir_str = self.skill_dir.to_string_lossy().replace('\\', "/");
        rendered = rendered.replace("${SKILL_DIR}", &dir_str);
        rendered = rendered.replace("${CLAUDE_SKILL_DIR}", &dir_str);

        rendered
    }

    /// Check if this skill should activate for a given file path
    pub fn matches_path(&self, file_path: &str) -> bool {
        if self.paths.is_empty() {
            return false; // No paths = unconditional (always active)
        }

        let normalized = file_path.replace('\\', "/");
        for pattern in &self.paths {
            if pattern.starts_with('!') {
                // Negation pattern
                let pat = &pattern[1..];
                if glob_match(pat, &normalized) {
                    return false;
                }
            } else if glob_match(pattern, &normalized) {
                return true;
            }
        }
        false
    }
}

/// Simple glob matching (supports *, **, ?)
fn glob_match(pattern: &str, path: &str) -> bool {
    globset::GlobBuilder::new(pattern)
        .literal_separator(false)
        .build()
        .ok()
        .map(|g| g.compile_matcher().is_match(path))
        .unwrap_or(false)
}
