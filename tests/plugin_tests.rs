mod manifest {
    #[test]
    fn test_parse_valid_manifest() {
        let json = r#"{"name":"test-plugin","version":"1.0.0","description":"A test"}"#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_validate_empty_name() {
        let m = Manifest { name: "".into(), version: "1.0.0".into() };
        assert!(validate(&m).is_err());
    }

    #[test]
    fn test_validate_name_with_spaces() {
        let m = Manifest { name: "bad name".into(), version: "1.0.0".into() };
        assert!(validate(&m).is_err());
    }

    #[test]
    fn test_validate_bad_version() {
        let m = Manifest { name: "ok".into(), version: "not-semver".into() };
        assert!(validate(&m).is_err());
    }

    #[test]
    fn test_validate_good() {
        let m = Manifest { name: "my-plugin".into(), version: "2.1.0".into() };
        assert!(validate(&m).is_ok());
    }

    #[derive(serde::Deserialize)]
    struct Manifest { name: String, version: String }

    fn validate(m: &Manifest) -> Result<(), String> {
        if m.name.is_empty() { return Err("name required".into()); }
        if m.name.contains(' ') || m.name.contains('/') { return Err("invalid name".into()); }
        let parts: Vec<&str> = m.version.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.parse::<u32>().is_err()) {
            return Err("invalid version".into());
        }
        Ok(())
    }
}

mod doctor {
    #[test]
    fn test_diag_status_icons() {
        assert_eq!(icon("ok"), "[ok]");
        assert_eq!(icon("warn"), "[warn]");
        assert_eq!(icon("error"), "[ERROR]");
    }

    #[test]
    fn test_check_count_summary() {
        let checks = vec![("ok", "a"), ("ok", "b"), ("warn", "c"), ("error", "d")];
        let ok = checks.iter().filter(|(s, _)| *s == "ok").count();
        let warn = checks.iter().filter(|(s, _)| *s == "warn").count();
        let err = checks.iter().filter(|(s, _)| *s == "error").count();
        assert_eq!(ok, 2);
        assert_eq!(warn, 1);
        assert_eq!(err, 1);
    }

    fn icon(status: &str) -> &'static str {
        match status {
            "ok" => "[ok]",
            "warn" => "[warn]",
            "error" => "[ERROR]",
            _ => "[?]",
        }
    }
}
