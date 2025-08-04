use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

const DEFAULT_MAX_RETRIES: u32 = 2;
const DEFAULT_TIMEOUT_SECS: u64 = 5;

/// Lightweight structure specifically for extracting the value field
#[derive(Debug, Deserialize)]
struct VaultValueOnly {
    value: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct VaultClient {
    pub domain: String,
    pub api_key: String,
    pub max_retries: u32,
    pub timeout_secs: u64,
    client: Client,
}

impl VaultClient {
    pub fn new(domain: String, api_key: String) -> Self {
        let client = Client::new();

        Self {
            domain,
            api_key,
            max_retries: DEFAULT_MAX_RETRIES,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            client,
        }
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Fetch vaultlet value by ID
    ///
    /// # Arguments
    ///
    /// * `id` - vaultlet ID
    ///
    /// # Returns
    ///
    /// Returns the value data from vaultlet as HashMap<String, String>
    ///
    /// # Example
    ///
    /// ```no_run
    /// use vault::VaultClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let client = VaultClient::new(
    ///         "http://127.0.0.1:7893".to_string(),
    ///         "your-api-key".to_string()
    ///     ).with_max_retries(5)
    ///      .with_timeout_secs(10);
    ///     
    ///     let value = client.fetch("vault-id".to_string()).await?;
    ///     println!("{:?}", value);
    ///     Ok(())
    /// }
    /// ```
    pub async fn fetch(&self, id: String) -> Result<HashMap<String, String>> {
        let start_time = Instant::now();
        let url = format!("{}/v1/vaultlet/{}", self.domain, id);
        let auth_header = format!("Bearer api-{}", self.api_key);

        for attempt in 0..=self.max_retries {
            let response = self
                .client
                .get(&url)
                .header("Authorization", &auth_header)
                .timeout(Duration::from_secs(self.timeout_secs))
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        log_request_duration(&id, start_time, "completed");
                        return parse_vaultlet_response(resp).await;
                    } else if should_retry(status, attempt, self.max_retries) {
                        warn!(
                            "Request failed with status {}, retrying... (attempt {}/{})",
                            status,
                            attempt + 1,
                            self.max_retries + 1
                        );
                        continue;
                    } else {
                        log_request_duration(
                            &id,
                            start_time,
                            &format!("failed with status: {}", status),
                        );
                        return Err(create_status_error(status, self.max_retries));
                    }
                }
                Err(e) => {
                    if attempt < self.max_retries {
                        warn!(
                            "Request failed with error: {}, retrying... (attempt {}/{})",
                            e,
                            attempt + 1,
                            self.max_retries + 1
                        );
                        continue;
                    } else {
                        log_request_duration(&id, start_time, &format!("failed with error: {}", e));
                        return Err(anyhow!(
                            "Request failed after {} retries: {}",
                            self.max_retries,
                            e
                        ));
                    }
                }
            }
        }

        unreachable!("Loop should have returned or continued")
    }
}

fn log_request_duration(id: &str, start_time: Instant, status: &str) {
    let elapsed = start_time.elapsed();
    let duration_secs = elapsed.as_secs_f64();

    if elapsed >= Duration::from_secs(2) {
        info!(
            "Vault request for ID '{}' {} in {:.2}s",
            id, status, duration_secs
        );
    } else {
        debug!(
            "Vault request for ID '{}' {} in {:.2}s",
            id, status, duration_secs
        );
    }
}

async fn parse_vaultlet_response(resp: reqwest::Response) -> Result<HashMap<String, String>> {
    let vault_data: VaultValueOnly = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse JSON response: {}", e))?;

    Ok(vault_data.value)
}

fn should_retry(status: reqwest::StatusCode, attempt: u32, max_retries: u32) -> bool {
    status.as_u16() >= 500 && attempt < max_retries
}

fn create_status_error(status: reqwest::StatusCode, max_retries: u32) -> anyhow::Error {
    if status.as_u16() >= 500 {
        anyhow!("Server error after {} retries: {}", max_retries, status)
    } else {
        anyhow!("Client error: {}", status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn test_vault_client_creation() {
        let client = VaultClient::new("http://127.0.0.1:7893".to_string(), "test-key".to_string());

        assert_eq!(client.domain, "http://127.0.0.1:7893");
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(client.timeout_secs, DEFAULT_TIMEOUT_SECS);

        let client_custom =
            VaultClient::new("http://example.com".to_string(), "custom-key".to_string())
                .with_max_retries(5)
                .with_timeout_secs(10);

        assert_eq!(client_custom.max_retries, 5);
        assert_eq!(client_custom.timeout_secs, 10);
    }

    #[tokio::test]
    async fn test_fetch_success() {
        let mock_server = MockServer::start().await;

        let mut expected_value = HashMap::new();
        expected_value.insert("key1".to_string(), "value1".to_string());
        expected_value.insert("key2".to_string(), "value2".to_string());

        let response_body = serde_json::json!({
            "value": expected_value
        });

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/test-id"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = VaultClient::new(mock_server.uri(), "test-key".to_string());

        let result = client.fetch("test-id".to_string()).await;
        assert!(result.is_ok());

        assert_eq!(result.unwrap(), expected_value);
    }

    #[tokio::test]
    async fn test_fetch_client_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/non-existent"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = VaultClient::new(mock_server.uri(), "test-key".to_string());

        // Test client error (should not retry)
        let result = client.fetch("non-existent".to_string()).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Client error: 404"));
    }

    #[tokio::test]
    async fn test_fetch_server_error_with_retry() {
        let mock_server = MockServer::start().await;

        let max_retries = DEFAULT_MAX_RETRIES;
        let total_attempts = max_retries + 1; // initial attempt + retries

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/server-error"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(ResponseTemplate::new(500))
            .expect(total_attempts as u64) // Should be called total_attempts times
            .mount(&mock_server)
            .await;

        let client = VaultClient::new(mock_server.uri(), "test-key".to_string());

        // Test server error (should retry and eventually fail)
        let result = client.fetch("server-error".to_string()).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains(&format!("Server error after {} retries: 500", max_retries)));
    }

    #[tokio::test]
    async fn test_fetch_server_error_then_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/retry-success"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let mut expected_value = HashMap::new();
        expected_value.insert("retried".to_string(), "success".to_string());

        let response_body = serde_json::json!({
            "value": expected_value
        });

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/retry-success"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = VaultClient::new(mock_server.uri(), "test-key".to_string());

        let result = client.fetch("retry-success".to_string()).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value.get("retried"), Some(&"success".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/invalid-json"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid json"))
            .mount(&mock_server)
            .await;

        let client = VaultClient::new(mock_server.uri(), "test-key".to_string());

        let result = client.fetch("invalid-json".to_string()).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to parse JSON response"));
    }

    #[tokio::test]
    async fn test_fetch_timeout() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/vaultlet/timeout-test"))
            .and(header("Authorization", "Bearer api-test-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&serde_json::json!({"value": {"test": "value"}}))
                    .set_delay(std::time::Duration::from_secs(2)),
            )
            .mount(&mock_server)
            .await;

        let client =
            VaultClient::new(mock_server.uri(), "test-key".to_string()).with_timeout_secs(1);

        let result = client.fetch("timeout-test".to_string()).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("timeout"));
    }
}
