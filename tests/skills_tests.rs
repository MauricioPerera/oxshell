mod parser {
    #[test]
    fn test_parse_list_brackets() {
        assert_eq!(parse_list("[a, b, c]"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_list_no_brackets() {
        assert_eq!(parse_list("a, b, c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_list_quotes() {
        assert_eq!(parse_list("[\"bash\", 'grep']"), vec!["bash", "grep"]);
    }

    #[test]
    fn test_parse_list_empty() {
        let result: Vec<String> = parse_list("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_list_single() {
        assert_eq!(parse_list("bash"), vec!["bash"]);
    }

    #[test]
    fn test_split_frontmatter_present() {
        let content = "---\nname: test\n---\nBody here";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("name: test"));
        assert_eq!(body, "Body here");
    }

    #[test]
    fn test_split_frontmatter_absent() {
        let content = "No frontmatter here";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_split_frontmatter_no_closing() {
        let content = "---\nname: test\nno closing delimiter";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
    }

    #[test]
    fn test_skill_render_positional() {
        assert_eq!(render("Fix $1 in $2", "bug main.rs"), "Fix bug in main.rs");
    }

    #[test]
    fn test_skill_render_arguments() {
        assert_eq!(render("Do: $ARGUMENTS", "all the things"), "Do: all the things");
    }

    #[test]
    fn test_skill_render_braced() {
        assert_eq!(render("Fix ${1} now", "bug"), "Fix bug now");
    }

    fn parse_list(value: &str) -> Vec<String> {
        let cleaned = value.trim_start_matches('[').trim_end_matches(']').trim();
        if cleaned.is_empty() { return Vec::new(); }
        cleaned.split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") { return (None, content); }
        let after_first = &trimmed[3..];
        if let Some(end) = after_first.find("\n---") {
            let fm = &after_first[..end];
            let body_start = 3 + end + 4;
            let body = if body_start < trimmed.len() { trimmed[body_start..].trim_start() } else { "" };
            (Some(fm), body)
        } else {
            (None, content)
        }
    }

    fn render(prompt: &str, args: &str) -> String {
        let mut rendered = prompt.to_string();
        rendered = rendered.replace("$ARGUMENTS", args);
        rendered = rendered.replace("${ARGUMENTS}", args);
        let parts: Vec<&str> = args.split_whitespace().collect();
        for (i, part) in parts.iter().enumerate() {
            let ph = format!("${}", i + 1);
            let ph_braced = format!("${{{}}}", i + 1);
            rendered = rendered.replace(&ph_braced, part);
            rendered = rendered.replace(&ph, part);
        }
        rendered
    }
}
