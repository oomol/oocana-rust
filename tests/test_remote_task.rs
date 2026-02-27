use assert_cmd::prelude::*;
use std::process::{Command, Stdio};

use serde_json::Value;
use user_task_client::mock;

fn oocana_cmd() -> Command {
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.stdin(Stdio::null());
    cmd
}

#[test]
fn remote_task_flow() {
    let server = mock::start(23579);

    let output = oocana_cmd()
        .args([
            "run",
            "examples/remote_task",
            "--task-api-url",
            &server.url(),
            "--search-paths",
            "examples/remote_task",
            "--report-to-console",
        ])
        .output()
        .expect("failed to run oocana");

    drop(server);

    assert!(output.status.success(), "oocana exited with failure");

    // Parse reporter JSON lines from stdout to verify all nodes actually ran.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let finished_nodes: Vec<String> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|msg| msg["type"] == "BlockFinished")
        .filter_map(|msg| {
            msg["stacks"]
                .as_array()
                .and_then(|s| s.last())
                .and_then(|level| level["node_id"].as_str().map(String::from))
        })
        .collect();

    for node_id in &["prepare", "echo-1", "verify"] {
        assert!(
            finished_nodes.iter().any(|n| n == node_id),
            "node '{node_id}' did not finish; finished nodes: {finished_nodes:?}"
        );
    }
}
