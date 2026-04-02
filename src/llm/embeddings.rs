use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use std::sync::Mutex;

const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4/accounts";
pub const EMBEDDING_MODEL: &str = "@cf/google/embeddinggemma-300m";
pub const EMBEDDING_DIM: usize = 768;
const MAX_BATCH_SIZE: usize = 100;
const MAX_RETRIES: u32 = 2;
const TIMEOUT_SECS: u64 = 30;
const CACHE_SIZE: usize = 256;

/// Trait for text → vector embedding providers.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed one or more texts into vectors. Returns one vector per input.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    /// Dimensionality of output vectors.
    #[allow(dead_code)]
    fn dim(&self) -> usize;
}

// ─── Workers AI Embedder ──────────────────────────────────

/// Real semantic embeddings via Cloudflare Workers AI.
pub struct WorkersAIEmbedder {
    client: reqwest::Client,
    cf_token: String,
    account_id: String,
    cache: Mutex<lru::LruCache<u64, Vec<f32>>>,
}

impl WorkersAIEmbedder {
    pub fn new(cf_token: String, account_id: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            cf_token,
            account_id,
            cache: Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(CACHE_SIZE).unwrap(),
            )),
        }
    }

    fn cache_key(text: &str) -> u64 {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(text.as_bytes());
        u64::from_le_bytes(hash[..8].try_into().unwrap())
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!(
            "{}/{}/ai/run/{}",
            CF_API_BASE, self.account_id, EMBEDDING_MODEL
        );

        let body = serde_json::json!({ "text": texts });

        let mut last_err = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
            }

            match self
                .client
                .post(&url)
                .bearer_auth(&self.cf_token)
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_default();
                        last_err = Some(anyhow::anyhow!("API error {status}: {text}"));
                        continue;
                    }

                    let json: serde_json::Value = resp
                        .json()
                        .await
                        .context("Failed to parse embedding response")?;

                    if json["success"].as_bool() != Some(true) {
                        let errors = &json["errors"];
                        bail!("Embedding API error: {errors}");
                    }

                    let data = json["result"]["data"]
                        .as_array()
                        .context("Missing result.data in embedding response")?;

                    let vectors: Vec<Vec<f32>> = data
                        .iter()
                        .map(|arr| {
                            arr.as_array()
                                .unwrap_or(&Vec::new())
                                .iter()
                                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                                .collect()
                        })
                        .collect();

                    if vectors.len() != texts.len() {
                        bail!(
                            "Embedding count mismatch: expected {}, got {}",
                            texts.len(),
                            vectors.len()
                        );
                    }

                    return Ok(vectors);
                }
                Err(e) => {
                    last_err = Some(e.into());
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Embedding failed after retries")))
    }
}

#[async_trait]
impl Embedder for WorkersAIEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Check cache for all texts
        let mut results = vec![None; texts.len()];
        let mut uncached_indices = Vec::new();
        let mut uncached_texts = Vec::new();

        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            for (i, text) in texts.iter().enumerate() {
                let key = Self::cache_key(text);
                if let Some(vec) = cache.get(&key) {
                    results[i] = Some(vec.clone());
                } else {
                    uncached_indices.push(i);
                    uncached_texts.push(text.clone());
                }
            }
        }

        // Fetch uncached embeddings in batches
        if !uncached_texts.is_empty() {
            let mut all_new = Vec::new();
            for chunk in uncached_texts.chunks(MAX_BATCH_SIZE) {
                let batch_result = self.embed_batch(&chunk.to_vec()).await?;
                all_new.extend(batch_result);
            }

            // Store in cache and fill results
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            for (idx, vec) in uncached_indices.iter().zip(all_new.into_iter()) {
                let key = Self::cache_key(&texts[*idx]);
                cache.put(key, vec.clone());
                results[*idx] = Some(vec);
            }
        }

        Ok(results.into_iter().map(|r| r.unwrap()).collect())
    }

    fn dim(&self) -> usize {
        EMBEDDING_DIM
    }
}

// ─── SHA256 Fallback Embedder ─────────────────────────────

/// Deterministic hash-based embeddings for offline/fallback use.
/// Not semantic — produces unrelated vectors for similar texts.
pub struct Sha256Embedder {
    dim: usize,
}

impl Sha256Embedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    fn hash_to_vector(text: &str, dim: usize) -> Vec<f32> {
        use sha2::{Digest, Sha256};
        let mut vector = vec![0.0f32; dim];
        let hash = Sha256::digest(text.as_bytes());
        for (i, byte) in hash.iter().enumerate() {
            if i < dim {
                vector[i] = (*byte as f32 / 255.0) * 2.0 - 1.0;
            }
        }
        if dim > 32 {
            for i in 32..dim {
                vector[i] = vector[i % 32] * 0.5 + vector[(i + 7) % 32] * 0.5;
            }
        }
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vector {
                *v /= norm;
            }
        }
        vector
    }
}

#[async_trait]
impl Embedder for Sha256Embedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| Self::hash_to_vector(t, self.dim))
            .collect())
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

// ─── Fallback Embedder ────────────────────────────────────

/// Tries real API embeddings first, falls back to SHA256 on error.
pub struct FallbackEmbedder {
    primary: WorkersAIEmbedder,
    fallback: Sha256Embedder,
}

impl FallbackEmbedder {
    pub fn new(cf_token: String, account_id: String) -> Self {
        Self {
            primary: WorkersAIEmbedder::new(cf_token, account_id),
            fallback: Sha256Embedder::new(EMBEDDING_DIM),
        }
    }
}

#[async_trait]
impl Embedder for FallbackEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        match self.primary.embed(texts).await {
            Ok(vectors) => Ok(vectors),
            Err(e) => {
                tracing::warn!("Embedding API failed, using SHA256 fallback: {e}");
                self.fallback.embed(texts).await
            }
        }
    }

    fn dim(&self) -> usize {
        EMBEDDING_DIM
    }
}
