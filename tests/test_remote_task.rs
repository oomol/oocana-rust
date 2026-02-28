use assert_cmd::prelude::*;
use std::process::{Command, Stdio};

use serde_json::Value;
use remote_job_client::mock;

fn oocana_cmd() -> Command {
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.stdin(Stdio::null());
    cmd
}

/// Strip ANSI escape sequences (e.g. `\x1b[2m`) so raw tracing output can be
/// parsed as JSON.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn remote_task_flow() {
    let server = mock::start(23579);

    let output = oocana_cmd()
        .args([
            "run",
            "examples/remote_task",
            "--remote-block-url",
            &server.url(),
            "--search-paths",
            "examples/remote_task",
            "--reporter",
            "--report-to-console",
        ])
        .output()
        .expect("failed to run oocana");

    drop(server);

    assert!(output.status.success(), "oocana exited with failure");

    // Parse reporter JSON lines from stdout to verify all nodes actually ran.
    // The tracing layer may wrap output in ANSI escape codes, so strip them first.
    let raw_stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = strip_ansi(&raw_stdout);
    let finished_nodes: Vec<String> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
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
            "node '{node_id}' did not finish; finished_nodes: {finished_nodes:?}\n\
             --- raw stdout ({} bytes) ---\n{}\n\
             --- raw stderr ({} bytes) ---\n{}",
            output.stdout.len(),
            raw_stdout,
            output.stderr.len(),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
