#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use std::process::{Command, Stdio};

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
        // linux/macos's key order is not same order.
        .stdout(predicate::str::contains(r#"block-1":["my_count"]"#))
        .stdout(predicate::str::contains(r#"block-2":["my_count"]"#))
        .success();
}
