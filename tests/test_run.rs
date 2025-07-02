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
fn run_flow_with_input() {
    Command::cargo_bin("oocana")
        .unwrap()
        .args([
            "run",
            "examples/input",
            "--input-values",
            "{\"block-1\": {\"my_count\": 1}, \"block-2\": {\"my_count\": 2}}",
        ])
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
