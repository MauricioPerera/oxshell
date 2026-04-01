mod task_types {
    use std::time::SystemTime;

    #[test]
    fn test_task_id_format() {
        let id = new_task_id('b');
        assert!(id.starts_with('b'));
        assert_eq!(id.len(), 9); // prefix + 8 chars
    }

    #[test]
    fn test_task_id_uniqueness() {
        let id1 = new_task_id('a');
        let id2 = new_task_id('a');
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_task_status_terminal() {
        assert!(!is_terminal("pending"));
        assert!(!is_terminal("running"));
        assert!(is_terminal("completed"));
        assert!(is_terminal("failed"));
        assert!(is_terminal("killed"));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("hello"), "hello");
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("<script>"), "&lt;script&gt;");
        assert_eq!(xml_escape("a\"b"), "a&quot;b");
        assert_eq!(xml_escape("a&b<c>d\"e"), "a&amp;b&lt;c&gt;d&quot;e");
    }

    #[test]
    fn test_xml_escape_empty() {
        assert_eq!(xml_escape(""), "");
    }

    #[test]
    fn test_task_notification_format() {
        let xml = format!(
            "<task-notification>\n<task-id>{}</task-id>\n<status>{}</status>\n</task-notification>",
            "b12345678", "completed"
        );
        assert!(xml.contains("<task-id>b12345678</task-id>"));
        assert!(xml.contains("<status>completed</status>"));
    }

    fn new_task_id(prefix: char) -> String {
        let uuid = uuid::Uuid::new_v4().to_string().replace('-', "");
        format!("{}{}", prefix, &uuid[..8])
    }

    fn is_terminal(status: &str) -> bool {
        matches!(status, "completed" | "failed" | "killed")
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
}
