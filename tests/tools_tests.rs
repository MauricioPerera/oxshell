mod bash_blocking {
    #[test]
    fn test_blocked_rm_rf() {
        assert!(is_blocked("rm -rf /").is_some());
        assert!(is_blocked("rm -rf /*").is_some());
        assert!(is_blocked("RM -RF /").is_some()); // case insensitive
    }

    #[test]
    fn test_blocked_dangerous_commands() {
        assert!(is_blocked("mkfs.ext4 /dev/sda").is_some());
        assert!(is_blocked("dd if=/dev/zero of=/dev/sda").is_some());
        assert!(is_blocked(":(){:|:&};:").is_some()); // fork bomb
        assert!(is_blocked("shutdown -h now").is_some());
        assert!(is_blocked("reboot").is_some());
    }

    #[test]
    fn test_blocked_evasion_eval() {
        assert!(is_blocked("eval 'rm -rf /'").is_some());
        assert!(is_blocked("eval\tcommand").is_some());
    }

    #[test]
    fn test_safe_commands() {
        assert!(is_blocked("ls -la").is_none());
        assert!(is_blocked("cat README.md").is_none());
        assert!(is_blocked("git status").is_none());
        assert!(is_blocked("cargo build").is_none());
        assert!(is_blocked("grep -r TODO src/").is_none());
    }

    #[test]
    fn test_safe_substrings() {
        // "rm" substring in safe context
        assert!(is_blocked("format_string").is_none());
        assert!(is_blocked("transform data").is_none());
        assert!(is_blocked("cargo rm-old-dep").is_none());
    }

    const BLOCKED_PATTERNS: &[&str] = &[
        "rm -rf /", "rm -rf /*", "mkfs.", "dd if=", ":(){:|:&};:",
        "> /dev/sda", "chmod -R 777 /", "curl|sh", "curl|bash",
        "wget|sh", "wget|bash", "shutdown", "reboot",
        "format c:", "del /f /s /q c:", "netsh advfirewall", "reg delete",
    ];

    fn is_blocked(command: &str) -> Option<&'static str> {
        let lower = command.to_lowercase();
        let compact = lower.replace(' ', "");

        for pattern in BLOCKED_PATTERNS {
            let pattern_compact = pattern.to_lowercase().replace(' ', "");
            if compact.contains(&pattern_compact) {
                return Some(pattern);
            }
        }

        if lower.contains("$(") && (compact.contains("rm-rf") || compact.contains("mkfs")) {
            return Some("shell expansion");
        }
        if lower.contains("eval ") || lower.contains("eval\t") {
            return Some("eval");
        }
        if lower.contains("\\x") && lower.contains("rm") {
            return Some("hex-encoded");
        }
        None
    }
}

mod permissions {
    use serde_json::json;

    #[test]
    fn test_validate_sensitive_paths() {
        assert!(!validate_write(".env"));
        assert!(!validate_write("/home/user/.ssh/id_rsa"));
        assert!(!validate_write("credentials.json"));
        assert!(!validate_write("/proc/self/environ"));
        assert!(!validate_write("C:\\Windows\\System32\\config"));
        assert!(!validate_write(".aws/credentials"));
    }

    #[test]
    fn test_validate_safe_paths() {
        assert!(validate_write("src/main.rs"));
        assert!(validate_write("README.md"));
        assert!(validate_write("output/results.json"));
        assert!(validate_write("tests/test_module.rs"));
    }

    fn validate_write(path: &str) -> bool {
        let lower = path.to_lowercase();
        let sensitive = [
            ".env", "credentials", ".ssh", "id_rsa", ".gnupg",
            "/proc/", "/sys/", "/dev/", "/var/run/secrets/",
            "\\system32\\", "\\windows\\config",
            "kubeconfig", ".kube/config",
            "azure/", "gcloud/", ".aws/",
        ];
        !sensitive.iter().any(|p| lower.contains(p))
    }
}
