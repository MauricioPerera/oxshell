use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use super::{Tool, ToolOutput};
use crate::a2e::{Workflow, execute_workflow};
use crate::permissions::ToolPermission;

/// Native A2E (Agent-to-Execution) tool.
/// Parses and executes JSONL workflows inline — no external server needed.
/// Supports: ApiCall, FilterData, TransformData, Conditional, Loop, StoreData, Wait, MergeData.
pub struct A2ETool;

#[async_trait]
impl Tool for A2ETool {
    fn name(&self) -> &str {
        "a2e_execute"
    }

    fn description(&self) -> &str {
        "Execute a declarative A2E workflow. Send JSONL with operations (ApiCall, FilterData, \
         TransformData, Conditional, Loop, StoreData, Wait, MergeData) that get validated and \
         executed locally. No external server needed.\n\n\
         Example JSONL:\n\
         {\"type\":\"operationUpdate\",\"operationId\":\"fetch\",\"operation\":{\"ApiCall\":{\"method\":\"GET\",\"url\":\"https://api.example.com/data\",\"outputPath\":\"/workflow/data\"}}}\n\
         {\"type\":\"operationUpdate\",\"operationId\":\"filter\",\"operation\":{\"FilterData\":{\"inputPath\":\"/workflow/data\",\"conditions\":[{\"field\":\"status\",\"operator\":\"==\",\"value\":\"active\"}],\"outputPath\":\"/workflow/active\"}}}\n\
         {\"type\":\"beginExecution\",\"executionId\":\"exec-1\",\"operationOrder\":[\"fetch\",\"filter\"]}"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "description": "JSONL workflow. Each line is a JSON object. Use operationUpdate to define steps and beginExecution to run them."
                },
                "validate_only": {
                    "type": "boolean",
                    "description": "If true, only parse and validate without executing",
                    "default": false
                }
            },
            "required": ["workflow"]
        })
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::RequiresApproval
    }

    async fn execute(&self, input: &Value) -> Result<ToolOutput> {
        let jsonl = input
            .get("workflow")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'workflow' parameter"))?;

        let validate_only = input
            .get("validate_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Parse JSONL into Workflow
        let workflow = match Workflow::parse(jsonl) {
            Ok(w) => w,
            Err(e) => return Ok(ToolOutput::error(format!("Workflow parse error: {e}"))),
        };

        if validate_only {
            return Ok(ToolOutput::success(format!(
                "Workflow valid: {} operations, execution order: [{}]",
                workflow.operations.len(),
                workflow.execution_order.join(", ")
            )));
        }

        // Execute workflow natively
        match execute_workflow(&workflow).await {
            Ok(result) => {
                let formatted = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| format!("{result:?}"));

                if result.success {
                    Ok(ToolOutput::success(formatted))
                } else {
                    Ok(ToolOutput::error(formatted))
                }
            }
            Err(e) => Ok(ToolOutput::error(format!("Workflow execution failed: {e}"))),
        }
    }
}
