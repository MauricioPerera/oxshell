use anyhow::{Context, Result, bail};
use reqwest::Client;
use tokio::sync::mpsc;

use super::streaming::handle_stream;
use super::types::*;

const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4/accounts";
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

pub struct WorkersAIClient {
    client: Client,
    cf_token: String, // Private — never expose in logs
    account_id: String,
    pub model: String,
    pub max_tokens: u32,
}

impl WorkersAIClient {
    pub fn new(
        cf_token: Option<String>,
        account_id: Option<String>,
        model: String,
    ) -> Result<Self> {
        let cf_token = cf_token
            .or_else(|| std::env::var("CLOUDFLARE_API_TOKEN").ok())
            .context(
                "Cloudflare API token required. Set CLOUDFLARE_API_TOKEN env var or pass --cf-token",
            )?;

        let account_id = account_id
            .or_else(|| std::env::var("CLOUDFLARE_ACCOUNT_ID").ok())
            .context(
                "Cloudflare Account ID required. Set CLOUDFLARE_ACCOUNT_ID env var or pass --account-id",
            )?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        Ok(Self {
            client,
            cf_token,
            account_id,
            model,
            max_tokens: 1024,
        })
    }

    /// Get a clone of credentials for spawning into async tasks.
    /// Returns (token, account_id, model) — avoid storing token longer than needed.
    pub fn credentials(&self) -> (String, String, String) {
        (
            self.cf_token.clone(),
            self.account_id.clone(),
            self.model.clone(),
        )
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/{}/ai/v1/chat/completions",
            CF_API_BASE, self.account_id
        )
    }

    /// Send a non-streaming request with retry logic
    pub async fn send_message(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<ChatCompletionResponse> {
        let mut all_messages = vec![Message::system(system.to_string())];
        all_messages.extend_from_slice(messages);

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: all_messages,
            tools: tools.to_vec(),
            max_tokens: Some(self.max_tokens),
            stream: false,
        };

        let mut last_error = String::new();

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    RETRY_DELAY_MS * (attempt as u64),
                ))
                .await;
                tracing::warn!("Retrying Workers AI request (attempt {}/{})", attempt + 1, MAX_RETRIES);
            }

            let result = self
                .client
                .post(&self.endpoint())
                .bearer_auth(&self.cf_token)
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let mut api_response: ChatCompletionResponse = response.json().await?;
                        // Normalize tool calls for models that embed them in content
                        // (e.g., Qwen uses <tools> tags instead of tool_calls array)
                        for choice in &mut api_response.choices {
                            if let Some(ref mut msg) = choice.message {
                                msg.normalize_tool_calls();
                            }
                        }
                        return Ok(api_response);
                    }

                    let body = response.text().await.unwrap_or_default();

                    // Don't retry on client errors (4xx) except 429 (rate limit)
                    if status.is_client_error() && status.as_u16() != 429 {
                        bail!("Workers AI error {status}: {body}");
                    }

                    last_error = format!("Workers AI error {status}: {body}");
                }
                Err(e) => {
                    last_error = format!("Network error: {e}");
                }
            }
        }

        bail!("Workers AI failed after {MAX_RETRIES} retries: {last_error}")
    }

    /// Send a streaming request (no retry — stream can't be replayed)
    pub async fn send_message_streaming(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<mpsc::Receiver<StreamEvent>> {
        let mut all_messages = vec![Message::system(system.to_string())];
        all_messages.extend_from_slice(messages);

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: all_messages,
            tools: tools.to_vec(),
            max_tokens: Some(self.max_tokens),
            stream: true,
        };

        let response = self
            .client
            .post(&self.endpoint())
            .bearer_auth(&self.cf_token)
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Workers AI")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("Workers AI error {status}: {body}");
        }

        let (tx, rx) = mpsc::channel(256);

        tokio::spawn(async move {
            if let Err(e) = handle_stream(response, &tx).await {
                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
            }
        });

        Ok(rx)
    }
}
