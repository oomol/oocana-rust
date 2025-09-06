#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
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
fn run_flow_with_input() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args([
            "run",
            "examples/input",
            "--nodes-inputs",
            "{\"block-1\": {\"my_count\": 1}, \"block-2\": {\"my_count\": 2}}",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn run_flow_with_absence_input() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "examples/input"])
        .stdin(Stdio::null())
        .assert()
        // linux macos's key order is not guaranteed to be the same, so we use contains to match the output
        .stdout(
            contains("these node won't run because some inputs are not provided: node(block-2) handles: [my_count], node(block-1) handles: [my_count]")
                .or(contains("these node won't run because some inputs are not provided: node(block-1) handles: [my_count], node(block-2) handles: [my_count]"))
        )
        .success();
}

// these test can not be test more strict like which is end node.
#[test]
fn condition_test() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args(["run", "examples/condition"])
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
