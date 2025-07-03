#[cfg(test)]
extern crate assert_cmd;
extern crate predicates;

use assert_cmd::prelude::*;

use std::{
    env::temp_dir,
    process::{Command, Stdio},
};

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
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    let tmp_file = temp_dir().join("oocana_test_query_input.json");

    cmd.args([
        "query",
        "input",
        "examples/input",
        "--output",
        tmp_file.to_str().unwrap(),
    ])
    .stdin(Stdio::null())
    .stderr(Stdio::inherit())
    .assert()
    .success();

    let result_str = std::fs::read_to_string(&tmp_file).expect("Failed to read output file");

    let map = serde_json::from_str::<serde_json::Value>(&result_str)
        .expect(format!("Failed to parse JSON output {}", result_str).as_str());
    println!("result: {}", result_str);

    assert!(map.is_object());
    assert!(
        map.get("block-1").is_some(),
        "Expected 'inputs' key in output"
    );

    for (key, value) in map.as_object().unwrap() {
        println!("{}: {}", key, value);
        assert!(
            value.is_array(),
            "Expected key({}) to have an array value, got {}",
            key,
            value
        );
        assert!(
            value.as_array().is_some_and(|v| v.len() == 2),
            "Expected key({}) to have only one element, got {}",
            key,
            value,
        );

        assert!(
            value.as_array().is_some_and(|v| v[0].is_object()),
            "Expected first element of array for {} to be an object",
            value
        );

        assert!(
            value
                .as_array()
                .is_some_and(|v| v[0].get("handle").is_some()),
            "Expected 'handle' key in first element of array for {}",
            key
        );

        for item in value.as_array().unwrap() {
            assert!(
                item.is_object(),
                "Expected each item in array for {} to be an object",
                key
            );
            println!("item: {}", item);
            if let Some(handle) = item.get("handle") {
                if handle.as_str().unwrap() == "in" {
                    assert!(
                        item.get("value").is_some_and(|v| v.is_number()),
                        "Expected handle [in] has number value, got: {}",
                        item
                    );
                } else if handle.as_str().unwrap() == "my_count" {
                    assert!(
                        item.get("value").is_none(),
                        "Expected handle [my_count] has no value, got: {}",
                        item
                    );
                } else {
                    panic!("Unexpected handle: {}", handle);
                }
            }
        }
    }
}
