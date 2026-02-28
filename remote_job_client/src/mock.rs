//! In-process mock task API server for testing.
//!
//! Enabled by the `mock` feature. Starts an axum HTTP server in a background
//! thread and returns a [`MockServer`] guard. The server shuts down when the
//! guard is dropped.

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
struct CreateTaskRequest {
    #[serde(rename = "type")]
    task_type: String,
    #[serde(rename = "inputValues")]
    input_values: Option<JsonMap>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskResponse {
    task_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskDetailResponse {
    task_type: String,
    task_id: String,
    status: String,
    progress: f64,
    created_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_message: Option<String>,
}

// -- Handlers ----------------------------------------------------------------

async fn create_task(
    State(tasks): State<Tasks>,
    Json(body): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let task_id = uuid::Uuid::new_v4().to_string();

    eprintln!("[mock] POST /tasks -> {task_id} (type={})", body.task_type);

    tasks.lock().unwrap().insert(
        task_id.clone(),
        Task {
            task_type: body.task_type,
            input_values: body.input_values,
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
            Json(serde_json::json!({ "message": format!("task {task_id} not found") })),
        );
    };

    let status = current_status(task);
    eprintln!("[mock] GET /tasks/{task_id} -> status={status}");

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
            Json(serde_json::json!({ "message": format!("task {task_id} not found") })),
        );
    };

    let status = current_status(task);
    eprintln!("[mock] GET /tasks/{task_id}/result -> status={status}");

    let resp = match status {
        "success" => {
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

// -- Public API --------------------------------------------------------------

/// Guard returned by [`start`]. Shuts down the mock server when dropped.
pub struct MockServer {
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub port: u16,
}

impl MockServer {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Start a mock task API server on `port`. Returns a [`MockServer`] guard.
///
/// The server runs in a background thread with its own tokio runtime and stops
/// when the returned guard is dropped.
///
/// Panics if the server does not become ready within 10 seconds.
pub fn start(port: u16) -> MockServer {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            let tasks: Tasks = Arc::new(Mutex::new(HashMap::new()));

            let app = Router::new()
                .route("/v3/users/me/tasks", post(create_task))
                .route("/v3/users/me/tasks/{task_id}", get(get_task_detail))
                .route(
                    "/v3/users/me/tasks/{task_id}/result",
                    get(get_task_result),
                )
                .with_state(tasks);

            let addr = format!("127.0.0.1:{port}");
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

            axum::serve(listener, app)
                .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
                .await
                .unwrap();
        });
    });

    // Wait until the server accepts connections.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() > deadline {
            panic!("mock server did not become ready within 10s");
        }
        if TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    MockServer {
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
        port,
    }
}
