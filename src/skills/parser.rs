use anyhow::{Context, Result};
use std::path::Path;

use super::types::*;

/// Parse a SKILL.md file into a Skill struct.
///
/// Format:
/// ```markdown
/// ---
/// name: my-skill
/// description: What this skill does
/// when_to_use: When to invoke this skill
/// arguments: $1
/// allowed-tools: [bash, file_read]
/// context: fork
/// user-invocable: true
/// paths: |
///   **/*.rs
///   !target/**
/// ---
///
/// # Prompt content here
/// You are an expert in $ARGUMENTS...
/// ```
pub fn parse_skill_file(path: &Path) -> Result<Skill> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

    let skill_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let dir_name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    parse_skill_content(&content, &dir_name, &skill_dir, SkillSource::Filesystem)
}

/// Parse skill content from a string (used for both file-based and bundled skills)
pub fn parse_skill_content(
    content: &str,
    default_name: &str,
    skill_dir: &Path,
    source: SkillSource,
) -> Result<Skill> {
    let (frontmatter, body) = split_frontmatter(content);

    let mut name = default_name.to_string();
    let mut description = String::new();
    let mut when_to_use = String::new();
    let mut arguments = Vec::new();
    let mut allowed_tools = Vec::new();
    let mut context = SkillContext::Inline;
    let mut user_invocable = true;
    let mut model_invocable = true;
    let mut paths = Vec::new();

    // Parse YAML-like frontmatter (simple key: value pairs)
    if let Some(fm) = frontmatter {
        for line in fm.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim().to_string();

                match key.as_str() {
                    "name" => name = value,
                    "description" => description = value,
                    "when_to_use" | "when-to-use" => when_to_use = value,
                    "arguments" | "argument" => {
                        arguments = parse_list(&value);
                    }
                    "allowed-tools" | "allowed_tools" => {
                        allowed_tools = parse_list(&value);
                    }
                    "context" if value == "fork" => context = SkillContext::Fork,
                    "user-invocable" | "user_invocable" => {
                        user_invocable = value != "false";
                    }
                    "disable-model-invocation" | "disable_model_invocation" => {
                        model_invocable = value != "true";
                    }
                    "paths" | "path" => {
                        // Paths can be multi-line (YAML block scalar)
                        // Handle inline: "**/*.rs, **/*.ts"
                        if !value.is_empty() && value != "|" {
                            paths.extend(parse_list(&value));
                        }
                    }
                    _ => {} // Ignore unknown fields
                }
            } else if !paths.is_empty() || line.starts_with("**") || line.starts_with("!") {
                // Multi-line paths continuation
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    paths.push(trimmed);
                }
            }
        }
    }

    // If no description, extract from first markdown paragraph
    if description.is_empty() {
        description = body
            .lines()
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .unwrap_or("")
            .chars()
            .take(120)
            .collect();
    }

    let is_conditional = !paths.is_empty();

    Ok(Skill {
        name,
        description,
        when_to_use,
        prompt: body.to_string(),
        arguments,
        allowed_tools,
        context,
        user_invocable,
        model_invocable,
        paths,
        source,
        skill_dir: skill_dir.to_path_buf(),
        active: !is_conditional, // Conditional skills start inactive
    })
}

/// Split content into optional frontmatter and body
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content);
    }

    // Find closing ---
    let after_first = &trimmed[3..];
    if let Some(end) = after_first.find("\n---") {
        let fm = &after_first[..end];
        let body_start = 3 + end + 4; // skip opening --- + fm + closing ---\n
        let body = if body_start < trimmed.len() {
            trimmed[body_start..].trim_start()
        } else {
            ""
        };
        (Some(fm), body)
    } else {
        (None, content)
    }
}

/// Parse a YAML-style list value: "[a, b, c]" or "a, b, c" or "a"
fn parse_list(value: &str) -> Vec<String> {
    let cleaned = value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();

    if cleaned.is_empty() {
        return Vec::new();
    }

    cleaned
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nname: test\ndescription: A test skill\n---\n\nHello $ARGUMENTS";
        let skill = parse_skill_content(content, "fallback", Path::new("."), SkillSource::Bundled)
            .unwrap();
        assert_eq!(skill.name, "test");
        assert_eq!(skill.description, "A test skill");
        assert_eq!(skill.prompt, "Hello $ARGUMENTS");
    }

    #[test]
    fn test_render_arguments() {
        let skill = Skill {
            name: "test".into(),
            description: "".into(),
            when_to_use: "".into(),
            prompt: "Fix $1 in ${2}".into(),
            arguments: vec!["$1".into(), "$2".into()],
            allowed_tools: vec![],
            context: SkillContext::Inline,
            user_invocable: true,
            model_invocable: true,
            paths: vec![],
            source: SkillSource::Bundled,
            skill_dir: Path::new(".").to_path_buf(),
            active: true,
        };
        assert_eq!(skill.render("bug main.rs"), "Fix bug in main.rs");
    }
}
