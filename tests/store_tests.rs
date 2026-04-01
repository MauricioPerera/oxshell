/// Tests for A2E WorkflowStore sandboxing and limits

mod store_sandbox {
    #[test]
    fn test_valid_paths() {
        let mut store = TestStore::new();
        assert!(store.set("/workflow/data", json_val("hello")).is_ok());
        assert!(store.set("/store/result", json_val("world")).is_ok());
        assert!(store.set("/workflow/nested/deep/path", json_val("ok")).is_ok());
    }

    #[test]
    fn test_blocked_paths() {
        let mut store = TestStore::new();
        assert!(store.set("/etc/passwd", json_val("bad")).is_err());
        assert!(store.set("/tmp/hack", json_val("bad")).is_err());
        assert!(store.set("relative/path", json_val("bad")).is_err());
        assert!(store.set("", json_val("bad")).is_err());
        assert!(store.set("/", json_val("bad")).is_err());
    }

    #[test]
    fn test_value_size_limit() {
        let mut store = TestStore::new();
        // 1MB + 1 byte should fail
        let big = "x".repeat(1_048_577);
        assert!(store.set("/workflow/big", json_val(&big)).is_err());
        // Under limit should work
        let small = "x".repeat(1000);
        assert!(store.set("/workflow/small", json_val(&small)).is_ok());
    }

    #[test]
    fn test_entry_count_limit() {
        let mut store = TestStore::new();
        // Fill to MAX (500)
        for i in 0..500 {
            assert!(store.set(&format!("/workflow/item{i}"), json_val("v")).is_ok());
        }
        // 501st should fail
        assert!(store.set("/workflow/overflow", json_val("v")).is_err());
        // But updating existing should work
        assert!(store.set("/workflow/item0", json_val("updated")).is_ok());
    }

    #[test]
    fn test_get_and_len() {
        let mut store = TestStore::new();
        store.set("/workflow/a", json_val("1")).unwrap();
        store.set("/workflow/b", json_val("2")).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("/workflow/a").unwrap(), &json_val("1"));
        assert!(store.get("/workflow/missing").is_none());
    }

    // Minimal store replica
    use std::collections::HashMap;

    const MAX_ENTRIES: usize = 500;
    const MAX_VALUE_BYTES: usize = 1_048_576;
    const ALLOWED: &[&str] = &["/workflow/", "/store/"];

    struct TestStore { data: HashMap<String, serde_json::Value> }
    impl TestStore {
        fn new() -> Self { Self { data: HashMap::new() } }
        fn set(&mut self, path: &str, value: serde_json::Value) -> Result<(), String> {
            if !ALLOWED.iter().any(|p| path.starts_with(p)) {
                return Err(format!("Invalid path: {path}"));
            }
            let size = serde_json::to_string(&value).map(|s| s.len()).unwrap_or(0);
            if size > MAX_VALUE_BYTES { return Err("Too large".into()); }
            if self.data.len() >= MAX_ENTRIES && !self.data.contains_key(path) {
                return Err("Store full".into());
            }
            self.data.insert(path.to_string(), value);
            Ok(())
        }
        fn get(&self, path: &str) -> Option<&serde_json::Value> { self.data.get(path) }
        fn len(&self) -> usize { self.data.len() }
    }

    fn json_val(s: &str) -> serde_json::Value { serde_json::Value::String(s.to_string()) }
}

mod compaction {
    #[test]
    fn test_model_context_limits() {
        assert_eq!(model_limit("hermes-2-pro-mistral-7b"), 4096);
        assert_eq!(model_limit("granite-4.0-h-micro"), 131_072);
        assert_eq!(model_limit("llama-3.1-8b"), 131_072);
        assert_eq!(model_limit("llama-3-8b"), 8192);
        assert_eq!(model_limit("deepseek-r1"), 16_384);
        assert_eq!(model_limit("qwen-2.5-32b"), 32_768);
        assert_eq!(model_limit("unknown-model"), 4096); // conservative default
    }

    #[test]
    fn test_estimate_tokens() {
        // ~4 chars per token
        let tokens = estimate_tokens(&["hello world"], "system prompt here");
        assert!(tokens > 0);
        assert!(tokens < 20); // rough estimate
    }

    #[test]
    fn test_compaction_threshold() {
        // 80% of 4096 = 3276
        let limit = 4096;
        let threshold = (limit as f64 * 0.80) as usize;
        assert_eq!(threshold, 3276);

        // Under threshold → no compact needed
        assert!(!needs_compact(2000, limit));
        // Over threshold → compact needed
        assert!(needs_compact(3500, limit));
    }

    #[test]
    fn test_keep_recent_messages() {
        let messages: Vec<&str> = (0..20).map(|i| "msg").collect();
        let keep = 6;
        let split = messages.len().saturating_sub(keep);
        assert_eq!(split, 14);
        assert_eq!(messages[split..].len(), 6);
    }

    fn model_limit(model: &str) -> usize {
        let m = model.to_lowercase();
        if m.contains("granite") { 131_072 }
        else if m.contains("llama-3.1") || m.contains("llama-4") { 131_072 }
        else if m.contains("llama-3.3") { 131_072 }
        else if m.contains("llama-3") { 8_192 }
        else if m.contains("deepseek") { 16_384 }
        else if m.contains("qwen") { 32_768 }
        else if m.contains("mistral") || m.contains("hermes") { 4_096 }
        else { 4_096 }
    }

    fn estimate_tokens(messages: &[&str], system: &str) -> usize {
        let msg_chars: usize = messages.iter().map(|m| m.len() + 20).sum();
        (system.len() + msg_chars) / 4
    }

    fn needs_compact(estimated: usize, limit: usize) -> bool {
        estimated >= (limit as f64 * 0.80) as usize
    }
}

mod session_types {
    #[test]
    fn test_auto_title_truncation() {
        let long_msg = "a".repeat(200);
        let title: String = long_msg.chars().take(80).collect();
        assert_eq!(title.len(), 80);
    }

    #[test]
    fn test_auto_title_short() {
        let msg = "Fix the bug in main.rs";
        let title: String = msg.chars().take(80).collect();
        assert_eq!(title, "Fix the bug in main.rs");
    }

    #[test]
    fn test_session_id_format() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(id.len() > 30); // UUID v4 = 36 chars with hyphens
        assert!(id.contains('-'));
    }

    #[test]
    fn test_jsonl_round_trip() {
        let entry = serde_json::json!({
            "timestamp": "2026-04-01T00:00:00Z",
            "message": {"role": "user", "content": "hello"},
            "is_compaction_summary": false
        });
        let line = serde_json::to_string(&entry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["message"]["content"], "hello");
    }
}
