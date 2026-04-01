use serde::Deserialize;
use std::collections::HashMap;

// Replicate the JSONL parser for testing (since oxshell is a binary crate)

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WorkflowMessage {
    OperationUpdate {
        #[serde(rename = "operationId")]
        operation_id: String,
        operation: serde_json::Value,
    },
    BeginExecution {
        #[serde(rename = "executionId")]
        execution_id: String,
        #[serde(rename = "operationOrder")]
        operation_order: Vec<String>,
    },
}

fn parse_workflow(jsonl: &str) -> Result<(String, Vec<String>, HashMap<String, serde_json::Value>), String> {
    let mut operations = HashMap::new();
    let mut execution_id = String::new();
    let mut execution_order = Vec::new();

    for (num, line) in jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() { continue; }

        let msg: WorkflowMessage = serde_json::from_str(line)
            .map_err(|e| format!("Line {}: {e}", num + 1))?;

        match msg {
            WorkflowMessage::OperationUpdate { operation_id, operation } => {
                operations.insert(operation_id, operation);
            }
            WorkflowMessage::BeginExecution { execution_id: eid, operation_order: order } => {
                execution_id = eid;
                execution_order = order;
            }
        }
    }

    if execution_order.is_empty() {
        return Err("Missing beginExecution message".to_string());
    }

    for op_id in &execution_order {
        if !operations.contains_key(op_id) {
            return Err(format!("Operation '{op_id}' referenced but not defined"));
        }
    }

    Ok((execution_id, execution_order, operations))
}

#[test]
fn test_workflow_parse_valid() {
    let jsonl = r#"{"type":"operationUpdate","operationId":"fetch","operation":{"ApiCall":{"method":"GET","url":"https://example.com","outputPath":"/workflow/data"}}}
{"type":"operationUpdate","operationId":"filter","operation":{"FilterData":{"inputPath":"/workflow/data","conditions":[{"field":"active","operator":"==","value":true}],"outputPath":"/workflow/filtered"}}}
{"type":"beginExecution","executionId":"exec-1","operationOrder":["fetch","filter"]}"#;

    let (eid, order, ops) = parse_workflow(jsonl).unwrap();
    assert_eq!(eid, "exec-1");
    assert_eq!(order, vec!["fetch", "filter"]);
    assert_eq!(ops.len(), 2);
}

#[test]
fn test_workflow_parse_missing_begin() {
    let jsonl = r#"{"type":"operationUpdate","operationId":"fetch","operation":{"ApiCall":{"method":"GET","url":"https://example.com","outputPath":"/workflow/data"}}}"#;
    let result = parse_workflow(jsonl);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing beginExecution"));
}

#[test]
fn test_workflow_parse_undefined_operation() {
    let jsonl = r#"{"type":"operationUpdate","operationId":"fetch","operation":{"ApiCall":{"method":"GET","url":"https://x.com","outputPath":"/data"}}}
{"type":"beginExecution","executionId":"exec-1","operationOrder":["fetch","nonexistent"]}"#;
    let result = parse_workflow(jsonl);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("nonexistent"));
}

#[test]
fn test_workflow_parse_empty_lines_ignored() {
    let jsonl = "\n{\"type\":\"operationUpdate\",\"operationId\":\"w\",\"operation\":{\"Wait\":{\"durationMs\":10}}}\n\n{\"type\":\"beginExecution\",\"executionId\":\"e\",\"operationOrder\":[\"w\"]}\n";
    let (_, order, _) = parse_workflow(jsonl).unwrap();
    assert_eq!(order, vec!["w"]);
}

#[test]
fn test_workflow_parse_invalid_json() {
    let result = parse_workflow("not json\n{\"type\":\"beginExecution\"}");
    assert!(result.is_err());
}

// ─── Filter Condition Tests ─────────────────────────────

fn evaluate_condition(item: &serde_json::Value, field: &str, op: &str, value: &serde_json::Value) -> bool {
    let field_val = item.get(field);
    match op {
        "==" => field_val == Some(value),
        "!=" => field_val != Some(value),
        ">" => field_val.and_then(|v| v.as_f64()).zip(value.as_f64()).map(|(a, b)| a > b).unwrap_or(false),
        ">=" => field_val.and_then(|v| v.as_f64()).zip(value.as_f64()).map(|(a, b)| a >= b).unwrap_or(false),
        "<" => field_val.and_then(|v| v.as_f64()).zip(value.as_f64()).map(|(a, b)| a < b).unwrap_or(false),
        "<=" => field_val.and_then(|v| v.as_f64()).zip(value.as_f64()).map(|(a, b)| a <= b).unwrap_or(false),
        "contains" => {
            if let (Some(serde_json::Value::String(s)), serde_json::Value::String(sub)) = (field_val, value) {
                s.contains(sub.as_str())
            } else { false }
        }
        "exists" => field_val.is_some(),
        _ => false,
    }
}

#[test]
fn test_condition_equals() {
    let item = serde_json::json!({"status": "active", "count": 5});
    assert!(evaluate_condition(&item, "status", "==", &serde_json::json!("active")));
    assert!(!evaluate_condition(&item, "status", "==", &serde_json::json!("inactive")));
}

#[test]
fn test_condition_not_equals() {
    let item = serde_json::json!({"status": "active"});
    assert!(evaluate_condition(&item, "status", "!=", &serde_json::json!("inactive")));
    assert!(!evaluate_condition(&item, "status", "!=", &serde_json::json!("active")));
}

#[test]
fn test_condition_numeric() {
    let item = serde_json::json!({"score": 85});
    assert!(evaluate_condition(&item, "score", ">", &serde_json::json!(80)));
    assert!(evaluate_condition(&item, "score", ">=", &serde_json::json!(85)));
    assert!(evaluate_condition(&item, "score", "<", &serde_json::json!(90)));
    assert!(evaluate_condition(&item, "score", "<=", &serde_json::json!(85)));
    assert!(!evaluate_condition(&item, "score", ">", &serde_json::json!(90)));
}

#[test]
fn test_condition_contains() {
    let item = serde_json::json!({"name": "John Doe"});
    assert!(evaluate_condition(&item, "name", "contains", &serde_json::json!("John")));
    assert!(!evaluate_condition(&item, "name", "contains", &serde_json::json!("Jane")));
}

#[test]
fn test_condition_exists() {
    let item = serde_json::json!({"name": "John"});
    assert!(evaluate_condition(&item, "name", "exists", &serde_json::json!(null)));
    assert!(!evaluate_condition(&item, "missing", "exists", &serde_json::json!(null)));
}

#[test]
fn test_condition_missing_field() {
    let item = serde_json::json!({"a": 1});
    assert!(!evaluate_condition(&item, "b", "==", &serde_json::json!(1)));
    assert!(!evaluate_condition(&item, "b", ">", &serde_json::json!(0)));
}
