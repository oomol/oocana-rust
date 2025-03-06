#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use std::process::Command;

#[test]
fn test_cli() {
    let mut cmd = Command::cargo_bin("vocana").expect("Calling binary failed");
    cmd.assert().failure();
}
