use assert_cmd::prelude::*;
use std::process::{Command, Stdio};

fn oocana_cmd() -> Command {
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.stdin(Stdio::null());
    cmd
}

#[test]
fn should_failed_without_subcommand() {
    oocana_cmd().assert().failure();
}

#[test]
fn cache_clear() {
    oocana_cmd()
        .args(["cache", "clear"])
        .assert()
        .success();
}
