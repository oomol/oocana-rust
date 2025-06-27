#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use std::process::{Command, Stdio};

#[test]
fn smoke_test() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "examples/base"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn self_flow_run() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "examples/self_block/flows/my_flow"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn run_shell_flow() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "examples/shell"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn version_pkg_test() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "examples/flows/pkg"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn should_failed_if_flow_not_exist() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "error_path"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .failure();
}

#[test]
fn query_services() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["query", "service", "examples/service/flow.oo.yaml"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn query_upstream() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["query", "upstream", "--nodes", "block-6", "examples/base"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn query_input() {
    use predicates::prelude::*;
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.args(["query", "input", "examples/input"])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .assert()
        .stdout(predicate::str::contains(
            r#"{"block-1":["my_count"],"block-2":["my_count"]}"#,
        ))
        .success();
}

#[test]
fn cache_clear() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["cache", "clear"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn should_failed_without_subcommand() {
    Command::cargo_bin("oocana")
        .expect("Calling binary failed")
        .assert()
        .failure();
}
