use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::Duration;
use thiserror::Error;

#[cfg(feature = "mock")]
pub mod mock;

type JsonMap = Map<String, Value>;

#[derive(Debug, Error)]
pub enum TaskClientError {
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api returned non-success status {status}: {message}")]
    ApiStatus {
        status: reqwest::StatusCode,
        message: String,
    },
    #[error("task {task_id} failed: {message}")]
    TaskFailed { task_id: String, message: String },
}

pub type Result<T> = std::result::Result<T, TaskClientError>;

#[derive(Clone)]
enum Auth {
    None,
    Token(String),
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::None => f.write_str("None"),
            Auth::Token(_) => f.write_str("Token(**redacted**)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserTaskClient {
    base_url: String,
    client: Client,
    auth: Auth,
}

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

impl UserTaskClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client,
            auth: Auth::None,
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Auth::Token(token.into());
        self
    }

    pub async fn create_user_task(&self, payload: &CreateTaskRequest) -> Result<String> {
        let url = format!("{}/v3/users/me/tasks", self.base_url);
        let req = self.client.post(url).json(payload);
        let resp = self.with_auth(req).send().await?;
        let resp = ensure_success(resp).await?;
        let body: CreateTaskResponse = resp.json().await?;
        Ok(body.task_id)
    }

    pub async fn get_task_detail(&self, task_id: &str) -> Result<TaskDetail> {
        let url = format!("{}/v3/users/me/tasks/{task_id}", self.base_url);
        let req = self.client.get(url);
        let resp = self.with_auth(req).send().await?;
        let resp = ensure_success(resp).await?;
        let body: TaskDetail = resp.json().await?;
        Ok(body)
    }

    pub async fn get_task_result(&self, task_id: &str) -> Result<TaskResult> {
        let url = format!("{}/v3/users/me/tasks/{task_id}/result", self.base_url);
        let req = self.client.get(url);
        let resp = self.with_auth(req).send().await?;
        let resp = ensure_success(resp).await?;
        let body: TaskResult = resp.json().await?;
        Ok(body)
    }

    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Auth::None => req,
            Auth::Token(token) => req.header("oomol-token", token),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskResponse {
    task_id: String,
}

/// Serverless task creation request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    #[serde(rename = "type")]
    task_type: &'static str,
    pub package_name: String,
    pub package_version: String,
    pub block_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_values: Option<JsonMap>,
}

impl CreateTaskRequest {
    pub fn new(
        package_name: String,
        package_version: String,
        block_name: String,
        input_values: Option<JsonMap>,
    ) -> Self {
        Self {
            task_type: "serverless",
            package_name,
            package_version,
            block_name,
            input_values,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Scheduling,
    Scheduled,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub status: TaskStatus,
    pub progress: f64,
    pub failed_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskResult {
    Success {
        #[serde(default, rename = "resultData")]
        result_data: Option<JsonMap>,
    },
    Failed {
        error: Option<String>,
    },
    #[serde(other)]
    Pending,
}

#[derive(Debug, Deserialize)]
struct ApiErrorMessage {
    message: Option<String>,
}

async fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }

    let message = match resp.json::<ApiErrorMessage>().await {
        Ok(body) => body
            .message
            .unwrap_or_else(|| "unexpected non-success response".to_string()),
        Err(_) => "unexpected non-success response".to_string(),
    };

    Err(TaskClientError::ApiStatus { status, message })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_payload_serializes() {
        let payload = CreateTaskRequest::new(
            "@oomol/pkg".to_string(),
            "1.0.0".to_string(),
            "main".to_string(),
            None,
        );

        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert_eq!(value["type"], "serverless");
        assert_eq!(value["packageName"], "@oomol/pkg");
        assert_eq!(value["packageVersion"], "1.0.0");
        assert_eq!(value["blockName"], "main");
    }

    #[test]
    fn task_result_success_deserializes() {
        let raw = serde_json::json!({
            "status": "success",
            "resultData": { "foo": "bar" }
        });
        let result: TaskResult = serde_json::from_value(raw).expect("result should deserialize");
        match result {
            TaskResult::Success { result_data } => {
                let data = result_data.unwrap();
                assert_eq!(data.get("foo").unwrap(), "bar");
            }
            _ => panic!("unexpected task result variant"),
        }
    }

    #[test]
    fn task_result_pending_deserializes() {
        let raw = serde_json::json!({
            "status": "running",
            "progress": 0.5
        });
        let result: TaskResult = serde_json::from_value(raw).expect("result should deserialize");
        assert!(matches!(result, TaskResult::Pending));
    }
}
