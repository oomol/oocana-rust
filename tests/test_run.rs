use assert_cmd::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::process::{Command, Stdio};

fn oocana_cmd() -> Command {
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.stdin(Stdio::null());
    cmd
}

fn run_flow(path: &str) {
    oocana_cmd().args(["run", path]).assert().success();
}

#[test]
fn smoke_test() {
    run_flow("examples/base");
}

#[test]
fn self_flow_run() {
    run_flow("examples/self_block/flows/my_flow");
}

#[test]
fn run_shell_flow() {
    run_flow("examples/shell");
}

#[test]
fn shell_flow_stdout_capture() {
    // Verify that shell executor captures stdout correctly
    oocana_cmd()
        .args(["run", "examples/shell"])
        .assert()
        .stdout(contains("stdout message"))
        .success();
}

#[test]
fn shell_flow_stderr_capture() {
    // all stderr messages are prefixed with "HandleName("stderr" to identify their source
    oocana_cmd()
        .args(["run", "examples/shell"])
        .assert()
        .stdout(contains("stderr message"))
        .stdout(contains("HandleName(\"stderr"))
        .success();
}

#[test]
fn shell_flow_env_injection() {
    // Verify that custom environment variables are injected
    oocana_cmd()
        .args(["run", "examples/shell"])
        .assert()
        .stdout(contains("hello_oocana"))
        .success();
}

#[test]
fn shell_flow_node_chaining() {
    // Verify that nodes chain correctly via cwd
    // setup creates dir, append nodes add content, read-and-stderr reads it
    oocana_cmd()
        .args(["run", "examples/shell"])
        .assert()
        .stdout(contains("Line 1: Created at"))
        .stdout(contains("Line 2: Custom="))
        .stdout(contains("Line 3: Host="))
        .stdout(contains("cleaned"))
        .success();
}

#[test]
fn run_flow_with_input() {
    oocana_cmd()
        .args([
            "run",
            "examples/input",
            "--nodes-inputs",
            r#"{"block-1": {"my_count": 1}, "block-2": {"my_count": 2}}"#,
        ])
        .assert()
        .success();
}

#[test]
fn run_flow_with_absence_input() {
    // linux/macos key order is not guaranteed, so match both possible orders
    oocana_cmd()
        .args(["run", "examples/input"])
        .assert()
        .stdout(
            contains("node(block-1) handles: [my_count]")
                .and(contains("node(block-2) handles: [my_count]")),
        )
        .success();
}

#[test]
fn condition_test() {
    run_flow("examples/condition");
}

#[test]
fn version_pkg_test() {
    run_flow("examples/flows/pkg");
}

#[test]
fn should_failed_if_flow_not_exist() {
    oocana_cmd().args(["run", "error_path"]).assert().failure();
}
