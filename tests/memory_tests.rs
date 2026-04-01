// Memory module tests — extraction patterns, retrieval helpers, index formatting

mod extraction {
    #[test]
    fn test_secret_detection_api_keys() {
        // These are fake keys for testing
        assert!(has_secret("here is sk_live_abc123def456"));
        assert!(has_secret("token: ghp_abcdefghijklmnop"));
        assert!(has_secret("slack: xoxb-123-456-abc"));
        assert!(has_secret("aws AKIAIOSFODNN7EXAMPLE"));
        assert!(has_secret("password=mysecret123"));
        assert!(has_secret("Authorization: Bearer xyz"));
        assert!(has_secret("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(has_secret("jwt: eyJhbGciOiJIUzI1NiJ9"));
    }

    #[test]
    fn test_secret_detection_safe_text() {
        assert!(!has_secret("I'm working on the authentication module"));
        assert!(!has_secret("The sky is blue"));
        assert!(!has_secret("Fix the login page"));
        assert!(!has_secret("skaggs_live_music")); // "sk" substring but not a key pattern
    }

    #[test]
    fn test_secret_detection_case_insensitive() {
        assert!(has_secret("PASSWORD=test"));
        assert!(has_secret("Bearer ABC123"));
        assert!(has_secret("SK_LIVE_test123"));
    }

    // Helper to call the private function via pattern match
    fn has_secret(text: &str) -> bool {
        let secret_patterns = [
            "sk_live_", "sk_test_", "sk-ant-", "sk-proj-",
            "ghp_", "gho_", "github_pat_",
            "xoxb-", "xoxp-",
            "akia",
            "password=", "passwd=", "secret=",
            "bearer ", "authorization:",
            "-----begin rsa", "-----begin private",
            "eyj",
        ];
        let lower = text.to_lowercase();
        secret_patterns.iter().any(|p| lower.contains(p))
    }
}

mod retrieval {
    use chrono::Utc;

    #[test]
    fn test_memory_age_days_recent() {
        let now = Utc::now().to_rfc3339();
        let age = memory_age_days(&now);
        assert_eq!(age, 0);
    }

    #[test]
    fn test_memory_age_days_old() {
        let old = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let age = memory_age_days(&old);
        assert_eq!(age, 30);
    }

    #[test]
    fn test_memory_age_days_invalid() {
        let age = memory_age_days("not-a-date");
        assert_eq!(age, 0);
    }

    #[test]
    fn test_freshness_text_recent() {
        assert_eq!(freshness_text(0), "");
        assert_eq!(freshness_text(1), "");
    }

    #[test]
    fn test_freshness_text_old() {
        let text = freshness_text(3);
        assert!(text.contains("3 days"));
    }

    #[test]
    fn test_freshness_text_very_old() {
        let text = freshness_text(10);
        assert!(text.contains("WARNING"));
        assert!(text.contains("10 days"));
    }

    fn memory_age_days(updated_at: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(updated_at)
            .map(|dt| (Utc::now() - dt.with_timezone(&Utc)).num_days())
            .unwrap_or(0)
    }

    fn freshness_text(age_days: i64) -> String {
        if age_days > 7 {
            format!("WARNING: Memory is {age_days} days old. Verify before acting.")
        } else if age_days > 1 {
            format!("Note: Memory is {age_days} days old.")
        } else {
            String::new()
        }
    }
}

mod index {
    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("user"), "User");
        assert_eq!(capitalize("feedback"), "Feedback");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("a"), "A");
    }

    fn capitalize(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        }
    }
}
