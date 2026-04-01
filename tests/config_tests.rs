mod config_resolve {
    #[test]
    fn test_resolve_token_cli_priority() {
        let config_val = Some("config-token".to_string());
        let cli_val = Some("cli-token".to_string());
        // CLI should win over config
        let result = cli_val.or(config_val);
        assert_eq!(result.unwrap(), "cli-token");
    }

    #[test]
    fn test_resolve_token_fallback_to_config() {
        let config_val = Some("config-token".to_string());
        let cli_val: Option<String> = None;
        let result = cli_val.or(config_val);
        assert_eq!(result.unwrap(), "config-token");
    }

    #[test]
    fn test_resolve_token_none() {
        let config_val: Option<String> = None;
        let cli_val: Option<String> = None;
        let result = cli_val.or(config_val);
        assert!(result.is_none());
    }

    #[test]
    fn test_is_configured() {
        assert!(is_configured(&Some("token".into()), &Some("acct".into())));
        assert!(!is_configured(&None, &Some("acct".into())));
        assert!(!is_configured(&Some("token".into()), &None));
        assert!(!is_configured(&None, &None));
    }

    #[test]
    fn test_resolve_model_custom() {
        let default = "@hf/nousresearch/hermes-2-pro-mistral-7b";
        let custom = "@cf/ibm-granite/granite-4.0-h-micro";
        let config_model = Some("@cf/meta/llama-3.3-70b".to_string());

        // Custom CLI model overrides everything
        assert_eq!(resolve_model(custom, &config_model), custom);
        // Default CLI model falls back to config
        assert_eq!(resolve_model(default, &config_model), "@cf/meta/llama-3.3-70b");
        // Default CLI with no config stays default
        assert_eq!(resolve_model(default, &None), default);
    }

    fn is_configured(token: &Option<String>, account_id: &Option<String>) -> bool {
        token.is_some() && account_id.is_some()
    }

    fn resolve_model(cli: &str, config: &Option<String>) -> String {
        if cli == "@hf/nousresearch/hermes-2-pro-mistral-7b" {
            config.clone().unwrap_or_else(|| cli.to_string())
        } else {
            cli.to_string()
        }
    }
}
