use anyhow::bail;
use std::collections::HashMap;

const MAX_STORE_ENTRIES: usize = 500;
const MAX_VALUE_BYTES: usize = 1_048_576; // 1MB per value

/// In-memory data store for workflow execution.
/// Paths MUST start with /workflow/ or /store/ (sandboxed).
pub struct WorkflowStore {
    data: HashMap<String, serde_json::Value>,
}

/// Allowed path prefixes for store operations
const ALLOWED_PREFIXES: &[&str] = &["/workflow/", "/store/"];

fn validate_path(path: &str) -> anyhow::Result<()> {
    if ALLOWED_PREFIXES.iter().any(|p| path.starts_with(p)) {
        Ok(())
    } else {
        bail!("Invalid store path '{path}': must start with /workflow/ or /store/")
    }
}

impl WorkflowStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Set a value at a sandboxed path
    pub fn set(&mut self, path: &str, value: serde_json::Value) -> anyhow::Result<()> {
        validate_path(path)?;

        let value_size = serde_json::to_string(&value).map(|s| s.len()).unwrap_or(0);
        if value_size > MAX_VALUE_BYTES {
            bail!("Value too large ({value_size} bytes > {MAX_VALUE_BYTES} max)");
        }
        if self.data.len() >= MAX_STORE_ENTRIES && !self.data.contains_key(path) {
            bail!("Store full ({MAX_STORE_ENTRIES} entries max)");
        }

        self.data.insert(path.to_string(), value);
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&serde_json::Value> {
        self.data.get(path)
    }

    pub fn get_cloned(&self, path: &str) -> Option<serde_json::Value> {
        self.data.get(path).cloned()
    }

    pub fn all(&self) -> &HashMap<String, serde_json::Value> {
        &self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}
