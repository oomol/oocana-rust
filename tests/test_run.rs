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
    oocana_cmd()
        .args(["run", path])
        .assert()
        .success();
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
    oocana_cmd()
        .args(["run", "error_path"])
        .assert()
        .failure();
}
