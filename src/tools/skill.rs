use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use super::{Tool, ToolOutput};
use crate::permissions::ToolPermission;

/// Tool that allows the model to invoke registered skills.
/// The skill registry is queried at schema-generation time to build the description.
pub struct SkillTool {
    /// Comma-separated list of available skill names (for description)
    pub available_skills: String,
}

impl SkillTool {
    pub fn new(skill_names: &[&str]) -> Self {
        Self {
            available_skills: skill_names.join(", "),
        }
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Execute a registered skill by name. Skills are reusable prompts for common tasks."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": format!("The skill name to execute. Available: {}", self.available_skills)
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments to pass to the skill"
                }
            },
            "required": ["skill"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, _input: &Value) -> Result<ToolOutput> {
        // Actual execution is handled by the App/main loop which has access to the SkillRegistry.
        // This tool just signals intent — the orchestrator intercepts it.
        Ok(ToolOutput::success(
            "Skill execution is handled by the orchestrator.".to_string(),
        ))
    }
}
