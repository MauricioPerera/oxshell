use std::collections::HashMap;

/// In-memory data store for workflow execution.
/// Paths like "/workflow/users" map to JSON values.
/// Supports persistent storage via minimemory integration.
pub struct WorkflowStore {
    data: HashMap<String, serde_json::Value>,
}

impl WorkflowStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Set a value at a path (e.g., "/workflow/users")
    pub fn set(&mut self, path: &str, value: serde_json::Value) {
        self.data.insert(path.to_string(), value);
    }

    /// Get a value at a path
    pub fn get(&self, path: &str) -> Option<&serde_json::Value> {
        self.data.get(path)
    }

    /// Get a cloned value (for operations that consume)
    pub fn get_cloned(&self, path: &str) -> Option<serde_json::Value> {
        self.data.get(path).cloned()
    }

    /// Get all data (for final result)
    pub fn all(&self) -> &HashMap<String, serde_json::Value> {
        &self.data
    }
}
