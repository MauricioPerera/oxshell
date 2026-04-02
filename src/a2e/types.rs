use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parsed A2E workflow ready for execution
#[derive(Debug, Clone)]
pub struct Workflow {
    pub operations: HashMap<String, Operation>,
    pub execution_order: Vec<String>,
    pub execution_id: String,
}

/// JSONL message types
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkflowMessage {
    OperationUpdate {
        #[serde(rename = "operationId")]
        operation_id: String,
        operation: OperationDef,
    },
    BeginExecution {
        #[serde(rename = "executionId")]
        execution_id: String,
        #[serde(rename = "operationOrder")]
        operation_order: Vec<String>,
    },
}

/// Operation definition wrapper (the inner operation type)
#[derive(Debug, Clone, Deserialize)]
pub enum OperationDef {
    ApiCall(ApiCallOp),
    FilterData(FilterDataOp),
    TransformData(TransformDataOp),
    Conditional(ConditionalOp),
    Loop(LoopOp),
    StoreData(StoreDataOp),
    Wait(WaitOp),
    MergeData(MergeDataOp),
}

// ─── Operations ─────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ApiCallOp {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(rename = "outputPath")]
    pub output_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterDataOp {
    #[serde(rename = "inputPath")]
    pub input_path: String,
    pub conditions: Vec<FilterCondition>,
    #[serde(rename = "outputPath")]
    pub output_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterCondition {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransformDataOp {
    #[serde(rename = "inputPath")]
    pub input_path: String,
    pub transform: String,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(rename = "outputPath")]
    pub output_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConditionalOp {
    pub condition: FilterCondition,
    #[serde(rename = "inputPath")]
    pub input_path: String,
    #[serde(rename = "thenOp")]
    pub then_op: Option<String>,
    #[serde(rename = "elseOp")]
    pub else_op: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoopOp {
    #[serde(rename = "inputPath")]
    pub input_path: String,
    #[serde(rename = "itemVar")]
    pub item_var: Option<String>,
    #[serde(rename = "bodyOp")]
    pub body_op: String,
    #[serde(rename = "outputPath")]
    pub output_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct StoreDataOp {
    #[serde(rename = "inputPath")]
    pub input_path: String,
    pub key: String,
    #[serde(default)]
    pub persistent: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitOp {
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeDataOp {
    pub sources: Vec<String>,
    #[serde(rename = "outputPath")]
    pub output_path: String,
    #[serde(default)]
    pub strategy: Option<String>,
}

// ─── Internal Types ─────────────────────────────────────

/// Resolved operation ready to execute
#[derive(Debug, Clone)]
pub struct Operation {
    pub id: String,
    pub def: OperationDef,
}

/// Result of a workflow execution
#[derive(Debug, Serialize)]
pub struct ExecutionResult {
    pub execution_id: String,
    pub success: bool,
    pub results: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub operations_completed: usize,
    pub duration_ms: u128,
}

// ─── Parser ─────────────────────────────────────────────

impl Workflow {
    /// Parse a JSONL string into a Workflow
    pub fn parse(jsonl: &str) -> Result<Self, String> {
        let mut operations = HashMap::new();
        let mut execution_id = String::new();
        let mut execution_order = Vec::new();

        for (line_num, line) in jsonl.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let msg: WorkflowMessage = serde_json::from_str(line)
                .map_err(|e| format!("Line {}: parse error: {e}", line_num + 1))?;

            match msg {
                WorkflowMessage::OperationUpdate {
                    operation_id,
                    operation,
                } => {
                    operations.insert(
                        operation_id.clone(),
                        Operation {
                            id: operation_id,
                            def: operation,
                        },
                    );
                }
                WorkflowMessage::BeginExecution {
                    execution_id: eid,
                    operation_order: order,
                } => {
                    execution_id = eid;
                    execution_order = order;
                }
            }
        }

        if execution_order.is_empty() {
            return Err("Missing beginExecution message".to_string());
        }

        // Validate all referenced operations exist
        for op_id in &execution_order {
            if !operations.contains_key(op_id) {
                return Err(format!("Operation '{op_id}' referenced but not defined"));
            }
        }

        Ok(Self {
            operations,
            execution_order,
            execution_id,
        })
    }
}
