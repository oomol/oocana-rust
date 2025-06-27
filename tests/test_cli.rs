#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use std::process::{Command, Stdio};

#[test]
fn should_failed_without_subcommand() {
    Command::cargo_bin("oocana")
        .expect("Calling binary failed")
        .assert()
        .failure();
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
