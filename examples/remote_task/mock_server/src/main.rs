use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

type JsonMap = Map<String, Value>;

struct Task {
    task_type: String,
    input_values: Option<JsonMap>,
    created_at: Instant,
}

type Tasks = Arc<Mutex<HashMap<String, Task>>>;

// Thresholds for automatic state progression (seconds since creation).
const RUNNING_AFTER: u64 = 2;
const SUCCESS_AFTER: u64 = 4;

fn current_status(task: &Task) -> &'static str {
    let elapsed = task.created_at.elapsed().as_secs();
    if elapsed >= SUCCESS_AFTER {
        "success"
    } else if elapsed >= RUNNING_AFTER {
        "running"
    } else {
        "queued"
    }
}

fn progress(task: &Task) -> f64 {
    let elapsed = task.created_at.elapsed().as_secs_f64();
    (elapsed / SUCCESS_AFTER as f64).min(1.0)
}

// -- Request / Response types ------------------------------------------------

#[derive(Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum CreateTaskRequest {
    #[serde(rename = "serverless")]
    Serverless {
        #[serde(rename = "packageName")]
        package_name: String,
        #[serde(rename = "packageVersion")]
        package_version: String,
        #[serde(rename = "blockName")]
        block_name: String,
        #[serde(rename = "inputValues")]
        input_values: Option<JsonMap>,
    },
    #[serde(rename = "applet")]
    Applet {
        #[serde(rename = "appletID")]
        applet_id: String,
        #[serde(rename = "inputValues")]
        input_values: Option<JsonMap>,
    },
    #[serde(rename = "api_applet")]
    ApiApplet {
        #[serde(rename = "appletID")]
        applet_id: String,
        #[serde(rename = "inputValues")]
        input_values: Option<JsonMap>,
    },
    #[serde(rename = "web_task")]
    WebTask {
        #[serde(rename = "projectID")]
        project_id: String,
        #[serde(rename = "blockName")]
        block_name: String,
        #[serde(rename = "inputValues")]
        input_values: Option<JsonMap>,
    },
}

impl CreateTaskRequest {
    fn task_type(&self) -> &str {
        match self {
            Self::Serverless { .. } => "serverless",
            Self::Applet { .. } => "applet",
            Self::ApiApplet { .. } => "api_applet",
            Self::WebTask { .. } => "web_task",
        }
    }

    fn input_values(self) -> Option<JsonMap> {
        match self {
            Self::Serverless { input_values, .. }
            | Self::Applet { input_values, .. }
            | Self::ApiApplet { input_values, .. }
            | Self::WebTask { input_values, .. } => input_values,
        }
    }
}

#[derive(Serialize)]
struct CreateTaskResponse {
    #[serde(rename = "taskId")]
    task_id: String,
}

#[derive(Serialize)]
struct TaskDetailResponse {
    #[serde(rename = "taskType")]
    task_type: String,
    #[serde(rename = "taskId")]
    task_id: String,
    status: String,
    progress: f64,
    #[serde(rename = "createdAt")]
    created_at: f64,
    #[serde(rename = "startTime", skip_serializing_if = "Option::is_none")]
    start_time: Option<f64>,
    #[serde(rename = "endTime", skip_serializing_if = "Option::is_none")]
    end_time: Option<f64>,
    #[serde(rename = "resultUrl", skip_serializing_if = "Option::is_none")]
    result_url: Option<String>,
    #[serde(rename = "failedMessage", skip_serializing_if = "Option::is_none")]
    failed_message: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

// -- Handlers ----------------------------------------------------------------

async fn create_task(
    State(tasks): State<Tasks>,
    Json(body): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let task_id = uuid::Uuid::new_v4().to_string();
    let task_type = body.task_type().to_owned();
    let input_values = body.input_values();

    println!("[mock] POST /tasks -> {task_id} (type={task_type})");

    tasks.lock().unwrap().insert(
        task_id.clone(),
        Task {
            task_type,
            input_values,
            created_at: Instant::now(),
        },
    );

    (StatusCode::OK, Json(CreateTaskResponse { task_id }))
}

async fn get_task_detail(
    State(tasks): State<Tasks>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let guard = tasks.lock().unwrap();
    let Some(task) = guard.get(&task_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::to_value(ErrorResponse {
                message: format!("task {task_id} not found"),
            })
            .unwrap()),
        );
    };

    let status = current_status(task);
    println!("[mock] GET /tasks/{task_id} -> status={status}");

    let resp = TaskDetailResponse {
        task_type: task.task_type.clone(),
        task_id: task_id.clone(),
        status: status.to_owned(),
        progress: progress(task),
        created_at: 0.0,
        start_time: if status != "queued" { Some(0.0) } else { None },
        end_time: if status == "success" {
            Some(0.0)
        } else {
            None
        },
        result_url: None,
        failed_message: None,
    };

    (StatusCode::OK, Json(serde_json::to_value(resp).unwrap()))
}

async fn get_task_result(
    State(tasks): State<Tasks>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let guard = tasks.lock().unwrap();
    let Some(task) = guard.get(&task_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "message": format!("task {task_id} not found")
            })),
        );
    };

    let status = current_status(task);
    println!("[mock] GET /tasks/{task_id}/result -> status={status}");

    let resp = match status {
        "success" => {
            // Echo back input_values as resultData.
            let result_data = task.input_values.clone().unwrap_or_default();
            serde_json::json!({
                "status": "success",
                "resultData": result_data,
            })
        }
        "failed" => serde_json::json!({
            "status": "failed",
            "error": "simulated failure",
        }),
        other => serde_json::json!({
            "status": other,
            "progress": progress(task),
        }),
    };

    (StatusCode::OK, Json(resp))
}

// -- Main --------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let tasks: Tasks = Arc::new(Mutex::new(HashMap::new()));

    let app = Router::new()
        .route("/v3/users/me/tasks", post(create_task))
        .route("/v3/users/me/tasks/{task_id}", get(get_task_detail))
        .route(
            "/v3/users/me/tasks/{task_id}/result",
            get(get_task_result),
        )
        .with_state(tasks);

    let port = std::env::var("PORT").unwrap_or_else(|_| "13579".to_string());
    let addr = format!("127.0.0.1:{port}");
    println!("[mock] listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
