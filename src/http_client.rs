use anyhow::{Context, Result};
use reqwest::{Client, Request, Response};
use std::time::Duration;

use crate::error::ApiError;

pub struct KiroHttpClient {
    client: Client,
    max_retries: u32,
    base_delay_ms: u64,
}

impl KiroHttpClient {
    pub fn new(
        max_connections: usize,
        connect_timeout: u64,
        request_timeout: u64,
        max_retries: u32,
    ) -> Result<Self> {
        let client = Client::builder()
            .pool_max_idle_per_host(max_connections)
            .connect_timeout(Duration::from_secs(connect_timeout))
            .timeout(Duration::from_secs(request_timeout))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            max_retries,
            base_delay_ms: 1000,
        })
    }

    pub async fn request_with_retry(&self, request: Request) -> Result<Response, ApiError> {
        self.request_with_retry_internal(request, true).await
    }

    #[allow(dead_code)]
    pub async fn request_no_retry(&self, request: Request) -> Result<Response, ApiError> {
        self.request_with_retry_internal(request, false).await
    }

    async fn request_with_retry_internal(
        &self,
        request: Request,
        enable_retry: bool,
    ) -> Result<Response, ApiError> {
        let max_retries = if enable_retry { self.max_retries } else { 0 };
        let mut attempt = 0;

        let method = request.method().clone();
        let url = request.url().clone();
        tracing::debug!(method = %method, url = %url, "Sending HTTP request");

        loop {
            let req = request.try_clone().ok_or_else(|| {
                ApiError::Internal(anyhow::anyhow!("Request body is not cloneable"))
            })?;

            let result = self.client.execute(req).await;

            match result {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        return Ok(response);
                    }

                    match status.as_u16() {
                        429 | 500..=599 => {
                            if attempt < max_retries {
                                let delay = self.calculate_backoff_delay(attempt);
                                tracing::warn!(
                                    "Received {}, retrying after {}ms (attempt {}/{})",
                                    status, delay, attempt + 1, max_retries
                                );
                                tokio::time::sleep(Duration::from_millis(delay)).await;
                                attempt += 1;
                                continue;
                            }
                        }
                        _ => {}
                    }

                    let error_text = response.text().await.unwrap_or_default();
                    tracing::error!(
                        status = status.as_u16(),
                        url = %url,
                        response_body = %error_text,
                        "HTTP request failed"
                    );
                    return Err(ApiError::KiroApiError {
                        status: status.as_u16(),
                        message: error_text,
                    });
                }

                Err(e) => {
                    let error_kind = if e.is_timeout() {
                        "timeout"
                    } else if e.is_connect() {
                        "connection_failed"
                    } else {
                        "unknown"
                    };

                    if attempt < max_retries {
                        let delay = self.calculate_backoff_delay(attempt);
                        tracing::warn!(
                            "Request failed ({}): {}, retrying after {}ms (attempt {}/{})",
                            error_kind, e, delay, attempt + 1, max_retries
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        attempt += 1;
                        continue;
                    }

                    tracing::error!(
                        error_kind = error_kind,
                        error = %e,
                        url = %url,
                        "HTTP request failed after all retries"
                    );
                    return Err(ApiError::Internal(anyhow::anyhow!(
                        "HTTP request failed: {} (kind: {})",
                        e, error_kind
                    )));
                }
            }
        }
    }

    fn calculate_backoff_delay(&self, attempt: u32) -> u64 {
        let delay = self.base_delay_ms * 2_u64.pow(attempt);
        let jitter = (delay as f64 * 0.1 * rand::random()) as u64;
        delay + jitter
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

mod rand {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;

    pub fn random() -> f64 {
        let state = RandomState::new();
        (state.hash_one(std::time::SystemTime::now()) % 1000) as f64 / 1000.0
    }
}
