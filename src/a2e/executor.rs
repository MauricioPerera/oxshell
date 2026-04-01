use anyhow::{Result, bail};
use std::collections::HashMap;
use std::time::Instant;

use super::store::WorkflowStore;
use super::types::*;

const MAX_LOOP_ITERATIONS: usize = 1000;
const API_CALL_TIMEOUT_SECS: u64 = 30;

/// Execute a parsed workflow, returning structured results.
/// This is the native Rust A2E executor — no external server needed.
pub async fn execute_workflow(workflow: &Workflow) -> Result<ExecutionResult> {
    let start = Instant::now();
    let mut store = WorkflowStore::new();
    let mut completed = 0;

    for op_id in &workflow.execution_order {
        let op = match workflow.operations.get(op_id) {
            Some(o) => o,
            None => {
                return Ok(ExecutionResult {
                    execution_id: workflow.execution_id.clone(),
                    success: false,
                    results: store.all().clone(),
                    error: Some(format!("Operation '{op_id}' not found")),
                    operations_completed: completed,
                    duration_ms: start.elapsed().as_millis(),
                });
            }
        };

        let result = execute_operation(op, &mut store, &workflow.operations).await;

        match result {
            Ok(()) => {
                completed += 1;
            }
            Err(e) => {
                return Ok(ExecutionResult {
                    execution_id: workflow.execution_id.clone(),
                    success: false,
                    results: store.all().clone(),
                    error: Some(format!("Operation '{}' failed: {e}", op.id)),
                    operations_completed: completed,
                    duration_ms: start.elapsed().as_millis(),
                });
            }
        }
    }

    Ok(ExecutionResult {
        execution_id: workflow.execution_id.clone(),
        success: true,
        results: store.all().clone(),
        error: None,
        operations_completed: completed,
        duration_ms: start.elapsed().as_millis(),
    })
}

fn execute_operation<'a>(
    op: &'a Operation,
    store: &'a mut WorkflowStore,
    all_ops: &'a HashMap<String, Operation>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        match &op.def {
            OperationDef::ApiCall(api) => exec_api_call(api, store).await,
            OperationDef::FilterData(filter) => exec_filter(filter, store),
            OperationDef::TransformData(transform) => exec_transform(transform, store),
            OperationDef::Conditional(cond) => exec_conditional(cond, store, all_ops).await,
            OperationDef::Loop(lp) => exec_loop(lp, store, all_ops).await,
            OperationDef::StoreData(sd) => exec_store_data(sd, store),
            OperationDef::Wait(wait) => exec_wait(wait).await,
            OperationDef::MergeData(merge) => exec_merge(merge, store),
        }
    })
}

// ─── ApiCall ────────────────────────────────────────────

async fn exec_api_call(op: &ApiCallOp, store: &mut WorkflowStore) -> Result<()> {
    let client = reqwest::Client::new();

    let method = match op.method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        other => bail!("Unsupported HTTP method: {other}"),
    };

    let mut request = client
        .request(method, &op.url)
        .timeout(std::time::Duration::from_secs(API_CALL_TIMEOUT_SECS));

    for (key, value) in &op.headers {
        request = request.header(key.as_str(), value.as_str());
    }

    if let Some(ref body) = op.body {
        request = request.json(body);
    }

    let response = request.send().await?;
    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    // Try JSON parse; if non-JSON (HTML error pages, etc.), wrap as error
    let body: serde_json::Value = serde_json::from_str(&response_text).unwrap_or_else(|_| {
        if status.is_success() {
            serde_json::json!({"text": response_text})
        } else {
            serde_json::json!({"error": format!("HTTP {}: {}", status.as_u16(), &response_text[..response_text.len().min(500)])})
        }
    });

    if !status.is_success() {
        bail!("API returned HTTP {}: {}", status.as_u16(), &response_text[..response_text.len().min(200)]);
    }

    let result = serde_json::json!({
        "status": status.as_u16(),
        "data": body,
    });

    store.set(&op.output_path, result);
    Ok(())
}

// ─── FilterData ─────────────────────────────────────────

fn exec_filter(op: &FilterDataOp, store: &mut WorkflowStore) -> Result<()> {
    let input = store
        .get_cloned(&op.input_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found", op.input_path))?;

    // Get the data array (might be nested under .data)
    let items = extract_array(&input)?;

    let filtered: Vec<serde_json::Value> = items
        .into_iter()
        .filter(|item| {
            op.conditions
                .iter()
                .all(|cond| evaluate_condition(item, cond))
        })
        .collect();

    store.set(&op.output_path, serde_json::Value::Array(filtered));
    Ok(())
}

// ─── TransformData ──────────────────────────────────────

fn exec_transform(op: &TransformDataOp, store: &mut WorkflowStore) -> Result<()> {
    let input = store
        .get_cloned(&op.input_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found", op.input_path))?;

    let result = match op.transform.as_str() {
        "sort" => {
            let mut items = extract_array(&input)?;
            if let Some(ref field) = op.field {
                items.sort_by(|a, b| {
                    let va = a.get(field).and_then(|v| v.as_str()).unwrap_or("");
                    let vb = b.get(field).and_then(|v| v.as_str()).unwrap_or("");
                    va.cmp(vb)
                });
            }
            serde_json::Value::Array(items)
        }
        "reverse" => {
            let mut items = extract_array(&input)?;
            items.reverse();
            serde_json::Value::Array(items)
        }
        "count" => {
            let items = extract_array(&input)?;
            serde_json::json!(items.len())
        }
        "flatten" => {
            let items = extract_array(&input)?;
            let flat: Vec<serde_json::Value> = items
                .into_iter()
                .flat_map(|item| {
                    if let serde_json::Value::Array(inner) = item {
                        inner
                    } else {
                        vec![item]
                    }
                })
                .collect();
            serde_json::Value::Array(flat)
        }
        "unique" => {
            let items = extract_array(&input)?;
            let mut seen = std::collections::HashSet::new();
            let unique: Vec<serde_json::Value> = items
                .into_iter()
                .filter(|item| {
                    let key = serde_json::to_string(item).unwrap_or_default();
                    seen.insert(key)
                })
                .collect();
            serde_json::Value::Array(unique)
        }
        "first" => {
            let items = extract_array(&input)?;
            items.into_iter().next().unwrap_or(serde_json::Value::Null)
        }
        "last" => {
            let items = extract_array(&input)?;
            items.into_iter().last().unwrap_or(serde_json::Value::Null)
        }
        "keys" => {
            if let serde_json::Value::Object(map) = &input {
                let keys: Vec<serde_json::Value> = map
                    .keys()
                    .map(|k| serde_json::Value::String(k.clone()))
                    .collect();
                serde_json::Value::Array(keys)
            } else {
                bail!("'keys' transform requires an object input");
            }
        }
        "values" => {
            if let serde_json::Value::Object(map) = &input {
                serde_json::Value::Array(map.values().cloned().collect())
            } else {
                bail!("'values' transform requires an object input");
            }
        }
        other => bail!("Unknown transform: '{other}'"),
    };

    store.set(&op.output_path, result);
    Ok(())
}

// ─── Conditional ────────────────────────────────────────

async fn exec_conditional(
    op: &ConditionalOp,
    store: &mut WorkflowStore,
    all_ops: &HashMap<String, Operation>,
) -> Result<()> {
    let input = store
        .get_cloned(&op.input_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found", op.input_path))?;

    let matched = evaluate_condition(&input, &op.condition);

    let target_op_id = if matched {
        op.then_op.as_deref()
    } else {
        op.else_op.as_deref()
    };

    if let Some(op_id) = target_op_id {
        if let Some(target) = all_ops.get(op_id) {
            execute_operation(target, store, all_ops).await?;
        } else {
            bail!("Conditional target operation '{op_id}' not found");
        }
    }

    Ok(())
}

// ─── Loop ───────────────────────────────────────────────

async fn exec_loop(
    op: &LoopOp,
    store: &mut WorkflowStore,
    all_ops: &HashMap<String, Operation>,
) -> Result<()> {
    let input = store
        .get_cloned(&op.input_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found", op.input_path))?;

    let items = extract_array(&input)?;

    if items.len() > MAX_LOOP_ITERATIONS {
        bail!(
            "Loop exceeds max iterations ({} > {MAX_LOOP_ITERATIONS})",
            items.len()
        );
    }

    let item_var = op.item_var.as_deref().unwrap_or("/workflow/_item");
    let mut results = Vec::new();

    for item in items {
        store.set(item_var, item);

        if let Some(body) = all_ops.get(&op.body_op) {
            execute_operation(body, store, all_ops).await?;
        }

        // Collect loop body output if it wrote to a known path
        if let Some(val) = store.get_cloned(item_var) {
            results.push(val);
        }
    }

    store.set(&op.output_path, serde_json::Value::Array(results));
    Ok(())
}

// ─── StoreData ──────────────────────────────────────────

fn exec_store_data(op: &StoreDataOp, store: &mut WorkflowStore) -> Result<()> {
    let value = store
        .get_cloned(&op.input_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found", op.input_path))?;

    // Store under the key path
    store.set(&format!("/store/{}", op.key), value);
    Ok(())
}

// ─── Wait ───────────────────────────────────────────────

async fn exec_wait(op: &WaitOp) -> Result<()> {
    let max_wait = 30_000; // 30 seconds max
    let duration = op.duration_ms.min(max_wait);
    tokio::time::sleep(std::time::Duration::from_millis(duration)).await;
    Ok(())
}

// ─── MergeData ──────────────────────────────────────────

fn exec_merge(op: &MergeDataOp, store: &mut WorkflowStore) -> Result<()> {
    let strategy = op.strategy.as_deref().unwrap_or("concat");

    match strategy {
        "concat" => {
            let mut merged = Vec::new();
            for source in &op.sources {
                if let Some(val) = store.get_cloned(source) {
                    match val {
                        serde_json::Value::Array(items) => merged.extend(items),
                        other => merged.push(other),
                    }
                }
            }
            store.set(&op.output_path, serde_json::Value::Array(merged));
        }
        "object" => {
            let mut merged = serde_json::Map::new();
            for source in &op.sources {
                if let Some(serde_json::Value::Object(obj)) = store.get_cloned(source) {
                    merged.extend(obj);
                }
            }
            store.set(&op.output_path, serde_json::Value::Object(merged));
        }
        other => bail!("Unknown merge strategy: '{other}'"),
    }

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────

fn extract_array(value: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
    match value {
        serde_json::Value::Array(items) => Ok(items.clone()),
        // Auto-extract from .data field (common API response pattern)
        serde_json::Value::Object(obj) => {
            if let Some(serde_json::Value::Array(items)) = obj.get("data") {
                Ok(items.clone())
            } else {
                Ok(vec![value.clone()])
            }
        }
        _ => Ok(vec![value.clone()]),
    }
}

fn evaluate_condition(item: &serde_json::Value, cond: &FilterCondition) -> bool {
    let field_val = item.get(&cond.field);

    match cond.operator.as_str() {
        "==" | "eq" => field_val == Some(&cond.value),
        "!=" | "ne" => field_val != Some(&cond.value),
        ">" | "gt" => {
            compare_numbers(field_val, &cond.value)
                .map(|ord| ord == std::cmp::Ordering::Greater)
                .unwrap_or(false)
        }
        ">=" | "gte" => {
            compare_numbers(field_val, &cond.value)
                .map(|ord| ord != std::cmp::Ordering::Less)
                .unwrap_or(false)
        }
        "<" | "lt" => {
            compare_numbers(field_val, &cond.value)
                .map(|ord| ord == std::cmp::Ordering::Less)
                .unwrap_or(false)
        }
        "<=" | "lte" => {
            compare_numbers(field_val, &cond.value)
                .map(|ord| ord != std::cmp::Ordering::Greater)
                .unwrap_or(false)
        }
        "contains" => {
            if let (Some(serde_json::Value::String(s)), serde_json::Value::String(substr)) =
                (field_val, &cond.value)
            {
                s.contains(substr.as_str())
            } else {
                false
            }
        }
        "exists" => field_val.is_some(),
        _ => false,
    }
}

fn compare_numbers(
    a: Option<&serde_json::Value>,
    b: &serde_json::Value,
) -> Option<std::cmp::Ordering> {
    let a_num = a?.as_f64()?;
    let b_num = b.as_f64()?;
    a_num.partial_cmp(&b_num)
}
