mod usage {
    #[test]
    fn test_usage_accumulate() {
        let mut total = Usage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 };
        let other = Usage { prompt_tokens: 20, completion_tokens: 10, total_tokens: 30 };
        total.accumulate(&other);
        assert_eq!(total.prompt_tokens, 30);
        assert_eq!(total.completion_tokens, 15);
        assert_eq!(total.total_tokens, 45);
    }

    #[test]
    fn test_usage_cost_zero() {
        let u = Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
        assert_eq!(u.format_cost(), "$0");
    }

    #[test]
    fn test_usage_cost_small() {
        let u = Usage { prompt_tokens: 100, completion_tokens: 50, total_tokens: 150 };
        let cost = u.format_cost();
        assert!(cost.starts_with("$0.00"));
    }

    #[test]
    fn test_usage_cost_large() {
        let u = Usage { prompt_tokens: 50000, completion_tokens: 50000, total_tokens: 100000 };
        let cost = u.format_cost();
        assert!(cost.starts_with("$1."));
    }

    #[derive(Default)]
    struct Usage {
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
    }

    impl Usage {
        fn accumulate(&mut self, other: &Usage) {
            self.prompt_tokens += other.prompt_tokens;
            self.completion_tokens += other.completion_tokens;
            self.total_tokens += other.total_tokens;
        }

        fn estimated_cost(&self) -> f64 {
            (self.total_tokens as f64) * 0.011 / 1_000.0
        }

        fn format_cost(&self) -> String {
            let cost = self.estimated_cost();
            if cost == 0.0 {
                "$0".to_string()
            } else if cost < 0.01 {
                format!("${:.4}", cost)
            } else {
                format!("${:.2}", cost)
            }
        }
    }
}
