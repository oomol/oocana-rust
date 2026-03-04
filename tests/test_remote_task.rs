use assert_cmd::prelude::*;
use std::process::{Command, Output, Stdio};

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

fn assert_remote_task_output(output: &Output, expected: &[(&str, Option<usize>)]) {
    assert!(
        output.status.success(),
        "oocana exited with failure\n--- stdout ({} bytes) ---\n{}\n--- stderr ({} bytes) ---\n{}",
        output.stdout.len(),
        String::from_utf8_lossy(&output.stdout),
        output.stderr.len(),
        String::from_utf8_lossy(&output.stderr),
    );

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

    for (node_id, expected_count) in expected {
        let count = finished_nodes.iter().filter(|n| n == node_id).count();
        if let Some(expected_count) = expected_count {
            assert!(
                count == *expected_count,
                "node '{node_id}' expected to finish {expected_count} time(s), \
                 but finished {count} time(s); finished_nodes: {finished_nodes:?}\n\
                 --- raw stdout ({} bytes) ---\n{}\n\
                 --- raw stderr ({} bytes) ---\n{}",
                output.stdout.len(),
                raw_stdout,
                output.stderr.len(),
                String::from_utf8_lossy(&output.stderr),
            );
        } else {
            assert!(
                count >= 1,
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

    assert_remote_task_output(
        &output,
        &[
            ("single-output", Some(1)),
            ("after-single-output", Some(2)),
            ("multi-outputs", Some(1)),
            ("after-multi-outputs", Some(2)),
        ],
    );
}

/// Run against real remote server. Skipped by default.
/// Requires `.env` with OOMOL_TOKEN and OOCANA_REMOTE_BLOCK_URL.
///
///     cargo test --test test_remote_task remote_task_flow_real -- --ignored
#[test]
#[ignore]
fn remote_task_flow_real() {
    let output = oocana_cmd()
        .args([
            "run",
            "examples/remote_task",
            "--search-paths",
            "examples/remote_task",
            "--env-file",
            ".env",
            "--reporter",
            "--report-to-console",
        ])
        .output()
        .expect("failed to run oocana");

    // Save full output to temp file for post-mortem debugging.
    let log_path = std::env::temp_dir().join("remote_task_flow_real.log");
    let mut log = String::new();
    log.push_str("--- stdout ---\n");
    log.push_str(&String::from_utf8_lossy(&output.stdout));
    log.push_str("\n--- stderr ---\n");
    log.push_str(&String::from_utf8_lossy(&output.stderr));
    std::fs::write(&log_path, &log).ok();
    eprintln!("Full output saved to: {}", log_path.display());

    // Remote tasks each emit 5 streaming BlockOutput events plus a final
    // result in BlockFinished, so downstream nodes are triggered 6 times.
    assert_remote_task_output(
        &output,
        &[
            ("single-output", Some(1)),
            ("after-single-output", Some(6)),
            ("multi-outputs", Some(1)),
            ("after-multi-outputs", Some(6)),
        ],
    );
}
