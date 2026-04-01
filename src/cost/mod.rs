use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-model pricing data (USD per million tokens)
#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub input_per_m: f64,
    pub output_per_m: f64,
}

/// Known Workers AI model pricing
fn get_pricing(model: &str) -> ModelPricing {
    let m = model.to_lowercase();

    // Workers AI pricing (approximate, updated 2026-Q1)
    if m.contains("gpt-oss") {
        ModelPricing { input_per_m: 0.20, output_per_m: 0.30 }
    } else if m.contains("nemotron") {
        ModelPricing { input_per_m: 0.50, output_per_m: 1.50 }
    } else if m.contains("granite") {
        ModelPricing { input_per_m: 0.017, output_per_m: 0.11 }
    } else if m.contains("llama-4") {
        ModelPricing { input_per_m: 0.05, output_per_m: 0.25 }
    } else if m.contains("llama-3.3-70b") {
        ModelPricing { input_per_m: 0.29, output_per_m: 2.25 }
    } else if m.contains("llama-3.1-8b") || m.contains("llama-3-8b") {
        ModelPricing { input_per_m: 0.011, output_per_m: 0.011 }
    } else if m.contains("qwen2.5-coder-32b") {
        ModelPricing { input_per_m: 0.66, output_per_m: 1.00 }
    } else if m.contains("qwen") {
        ModelPricing { input_per_m: 0.011, output_per_m: 0.011 }
    } else if m.contains("deepseek") {
        ModelPricing { input_per_m: 0.011, output_per_m: 0.011 }
    } else if m.contains("mistral-small-3") {
        ModelPricing { input_per_m: 0.35, output_per_m: 0.56 }
    } else if m.contains("hermes") || m.contains("mistral") {
        ModelPricing { input_per_m: 0.011, output_per_m: 0.011 }
    } else {
        // Default: Workers AI neuron-based pricing
        ModelPricing { input_per_m: 0.011, output_per_m: 0.011 }
    }
}

/// Tracks costs across a session with per-model granularity
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostTracker {
    /// Per-model usage
    pub by_model: HashMap<String, ModelUsage>,
    /// Session budget limit (if set)
    pub budget: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub requests: u32,
    pub tool_calls: u32,
}

impl CostTracker {
    pub fn new(budget: Option<f64>) -> Self {
        Self {
            by_model: HashMap::new(),
            budget,
        }
    }

    /// Record usage from an API response
    pub fn record(&mut self, model: &str, input_tokens: u32, output_tokens: u32, tool_calls: u32) {
        let entry = self.by_model.entry(model.to_string()).or_default();
        entry.input_tokens += input_tokens as u64;
        entry.output_tokens += output_tokens as u64;
        entry.total_tokens += (input_tokens + output_tokens) as u64;
        entry.requests += 1;
        entry.tool_calls += tool_calls;
    }

    /// Calculate total cost across all models
    pub fn total_cost(&self) -> f64 {
        self.by_model
            .iter()
            .map(|(model, usage)| {
                let pricing = get_pricing(model);
                let input_cost = usage.input_tokens as f64 * pricing.input_per_m / 1_000_000.0;
                let output_cost = usage.output_tokens as f64 * pricing.output_per_m / 1_000_000.0;
                input_cost + output_cost
            })
            .sum()
    }

    /// Check if budget exceeded
    pub fn is_over_budget(&self) -> bool {
        if let Some(budget) = self.budget {
            self.total_cost() > budget
        } else {
            false
        }
    }

    /// Total tokens across all models
    pub fn total_tokens(&self) -> u64 {
        self.by_model.values().map(|u| u.total_tokens).sum()
    }

    /// Total requests
    pub fn total_requests(&self) -> u32 {
        self.by_model.values().map(|u| u.requests).sum()
    }

    /// Format cost for display
    pub fn format_cost(&self) -> String {
        let cost = self.total_cost();
        if cost == 0.0 {
            "$0".to_string()
        } else if cost < 0.01 {
            format!("${:.4}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }

    /// Format detailed breakdown
    pub fn format_breakdown(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "Total: {} ({} tokens, {} requests)",
            self.format_cost(),
            self.total_tokens(),
            self.total_requests()
        ));

        if self.by_model.len() > 1 {
            lines.push(String::new());
            for (model, usage) in &self.by_model {
                let pricing = get_pricing(model);
                let cost = usage.input_tokens as f64 * pricing.input_per_m / 1_000_000.0
                    + usage.output_tokens as f64 * pricing.output_per_m / 1_000_000.0;
                let short_name = model.split('/').last().unwrap_or(model);
                lines.push(format!(
                    "  {short_name}: ${cost:.4} ({} in / {} out, {} calls)",
                    usage.input_tokens, usage.output_tokens, usage.requests
                ));
            }
        }

        if let Some(budget) = self.budget {
            let remaining = budget - self.total_cost();
            if remaining > 0.0 {
                lines.push(format!("Budget: ${remaining:.4} remaining of ${budget:.2}"));
            } else {
                lines.push(format!("BUDGET EXCEEDED: ${:.4} over ${budget:.2}", -remaining));
            }
        }

        lines.join("\n")
    }
}
