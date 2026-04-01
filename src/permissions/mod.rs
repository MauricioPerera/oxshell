use serde_json::Value;
use std::collections::HashSet;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermission {
    AutoApprove,
    RequiresApproval,
}

pub struct PermissionManager {
    auto_approve_all: bool,
    session_approved: Mutex<HashSet<String>>,
    allowed_tools: Mutex<HashSet<String>>,
}

impl PermissionManager {
    pub fn new(auto_approve_all: bool) -> Self {
        if auto_approve_all {
            tracing::warn!("Auto-approve enabled — all tool executions will be allowed without confirmation");
        }
        Self {
            auto_approve_all,
            session_approved: Mutex::new(HashSet::new()),
            allowed_tools: Mutex::new(HashSet::new()),
        }
    }

    /// Check if a tool execution is allowed.
    /// Also performs basic input validation for dangerous tools.
    pub fn check(&self, tool_name: &str, permission: ToolPermission, input: &Value) -> bool {
        // Always validate dangerous inputs even in auto-approve mode
        if !self.validate_input(tool_name, input) {
            return false;
        }

        match permission {
            ToolPermission::AutoApprove => true,
            ToolPermission::RequiresApproval => {
                if self.auto_approve_all {
                    return true;
                }
                let approved = self.session_approved.lock().unwrap();
                if approved.contains(tool_name) {
                    return true;
                }
                let allowed = self.allowed_tools.lock().unwrap();
                allowed.contains(tool_name)
            }
        }
    }

    /// Basic input validation for dangerous tools
    fn validate_input(&self, tool_name: &str, input: &Value) -> bool {
        match tool_name {
            "file_write" | "file_edit" => {
                // Block writes to known sensitive files
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    let lower = path.to_lowercase();
                    let sensitive = [
                        ".env", "credentials", ".ssh", "id_rsa", ".gnupg",
                        "/proc/", "/sys/", "/dev/", "/var/run/secrets/",
                        "\\system32\\", "\\windows\\config",
                        "kubeconfig", ".kube/config",
                        "azure/", "gcloud/", ".aws/",
                    ];
                    if sensitive.iter().any(|p| lower.contains(p))
                    {
                        tracing::warn!("Blocked write to sensitive path: {path}");
                        return false;
                    }
                }
                true
            }
            _ => true,
        }
    }

    pub fn approve_session(&self, tool_name: &str) {
        let mut approved = self.session_approved.lock().unwrap();
        approved.insert(tool_name.to_string());
    }

    pub fn approve_always(&self, tool_name: &str) {
        let mut allowed = self.allowed_tools.lock().unwrap();
        allowed.insert(tool_name.to_string());
    }

    pub fn needs_approval(&self, tool_name: &str, permission: ToolPermission) -> bool {
        match permission {
            ToolPermission::AutoApprove => false,
            ToolPermission::RequiresApproval => {
                if self.auto_approve_all {
                    return false;
                }
                let approved = self.session_approved.lock().unwrap();
                if approved.contains(tool_name) {
                    return false;
                }
                let allowed = self.allowed_tools.lock().unwrap();
                !allowed.contains(tool_name)
            }
        }
    }
}
