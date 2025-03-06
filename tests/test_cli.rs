#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use std::process::{Command, Stdio};

#[test]
fn smoke_test() {
    Command::cargo_bin("vocana")
        .unwrap()
        .args(["run", "examples/base"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn should_failed_if_flow_not_exist() {
    Command::cargo_bin("vocana")
        .unwrap()
        .args(["run", "error_path"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .failure();
}

#[test]
fn should_failed_without_subcommand() {
    Command::cargo_bin("vocana")
        .expect("Calling binary failed")
        .assert()
        .failure();
}
