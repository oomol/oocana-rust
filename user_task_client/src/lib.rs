use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;

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
    #[error("task {task_id} timed out after {elapsed:?}")]
    Timeout { task_id: String, elapsed: Duration },
}

pub type Result<T> = std::result::Result<T, TaskClientError>;

#[derive(Clone)]
pub enum Auth {
    None,
    Bearer(String),
    Header { name: String, value: String },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::None => f.write_str("None"),
            Auth::Bearer(_) => f.write_str("Bearer(**redacted**)"),
            Auth::Header { name, .. } => f
                .debug_struct("Header")
                .field("name", name)
                .field("value", &"**redacted**")
                .finish(),
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

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Auth::Bearer(token.into());
        self
    }

    pub fn with_header_auth(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.auth = Auth::Header {
            name: name.into(),
            value: value.into(),
        };
        self
    }

    pub fn with_reqwest_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    pub async fn create_user_task(&self, payload: &CreateUserTaskRequest) -> Result<String> {
        let url = format!("{}/v3/users/me/tasks", self.base_url);
        let req = self.client.post(url).json(payload);
        let resp = self.with_auth(req).send().await?;
        let resp = ensure_success(resp).await?;
        let body: CreateUserTaskResponse = resp.json().await?;
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

    /// Create a task and poll until it reaches a terminal status, then fetch final result.
    pub async fn run_task(
        &self,
        payload: &CreateUserTaskRequest,
        options: RunTaskOptions,
    ) -> Result<RunTaskOutput> {
        let task_id = self.create_user_task(payload).await?;
        let started_at = Instant::now();

        loop {
            if let Some(timeout) = options.timeout {
                let elapsed = started_at.elapsed();
                if elapsed > timeout {
                    return Err(TaskClientError::Timeout { task_id, elapsed });
                }
            }

            let detail = self.get_task_detail(&task_id).await?;
            match detail.status {
                TaskStatus::Success => {
                    let result = self.get_task_result(&task_id).await?;
                    return Ok(RunTaskOutput {
                        task_id,
                        detail,
                        result,
                    });
                }
                TaskStatus::Failed => {
                    let fallback = detail
                        .failed_message
                        .clone()
                        .unwrap_or_else(|| "task failed".to_string());
                    let message = match self.get_task_result(&task_id).await {
                        Ok(TaskResult::Failed { error, .. }) => error.unwrap_or(fallback),
                        _ => fallback,
                    };
                    return Err(TaskClientError::TaskFailed { task_id, message });
                }
                _ => tokio::time::sleep(options.poll_interval).await,
            }
        }
    }

    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Auth::None => req,
            Auth::Bearer(token) => req.bearer_auth(token),
            Auth::Header { name, value } => req.header(name, value),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserTaskResponse {
    task_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum CreateUserTaskRequest {
    #[serde(rename = "serverless")]
    Serverless {
        #[serde(rename = "packageName")]
        package_name: String,
        #[serde(rename = "packageVersion")]
        package_version: String,
        #[serde(rename = "blockName")]
        block_name: String,
        #[serde(rename = "inputValues", skip_serializing_if = "Option::is_none")]
        input_values: Option<JsonMap>,
    },
    #[serde(rename = "applet")]
    Applet {
        #[serde(rename = "appletID")]
        applet_id: String,
        #[serde(rename = "inputValues", skip_serializing_if = "Option::is_none")]
        input_values: Option<JsonMap>,
    },
    #[serde(rename = "api_applet")]
    ApiApplet {
        #[serde(rename = "appletID")]
        applet_id: String,
        #[serde(rename = "inputValues", skip_serializing_if = "Option::is_none")]
        input_values: Option<JsonMap>,
    },
    #[serde(rename = "web_task")]
    WebTask {
        #[serde(rename = "projectID")]
        project_id: String,
        #[serde(rename = "blockName")]
        block_name: String,
        #[serde(rename = "inputValues", skip_serializing_if = "Option::is_none")]
        input_values: Option<JsonMap>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Scheduling,
    Scheduled,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub task_type: String,
    pub task_id: String,
    pub status: TaskStatus,
    pub progress: f64,
    pub created_at: f64,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub result_url: Option<String>,
    pub failed_message: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskResult {
    Queued {
        progress: f64,
    },
    Scheduling {
        progress: f64,
    },
    Scheduled {
        progress: f64,
    },
    Running {
        progress: f64,
    },
    Success {
        #[serde(rename = "resultURL")]
        result_url: Option<String>,
        #[serde(default, rename = "resultData")]
        result_data: Option<Vec<JsonMap>>,
    },
    Failed {
        error: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct RunTaskOptions {
    pub poll_interval: Duration,
    pub timeout: Option<Duration>,
}

impl Default for RunTaskOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            timeout: Some(Duration::from_secs(600)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunTaskOutput {
    pub task_id: String,
    pub detail: TaskDetail,
    pub result: TaskResult,
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
    fn create_serverless_payload_serializes() {
        let payload = CreateUserTaskRequest::Serverless {
            package_name: "@oomol/pkg".to_string(),
            package_version: "1.0.0".to_string(),
            block_name: "main".to_string(),
            input_values: None,
        };

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
            "resultURL": "https://example.com/result.json",
            "resultData": [{ "foo": "bar" }]
        });
        let result: TaskResult = serde_json::from_value(raw).expect("result should deserialize");
        match result {
            TaskResult::Success {
                result_url,
                result_data,
            } => {
                assert_eq!(
                    result_url.as_deref(),
                    Some("https://example.com/result.json")
                );
                assert_eq!(result_data.unwrap().len(), 1);
            }
            _ => panic!("unexpected task result variant"),
        }
    }
}
