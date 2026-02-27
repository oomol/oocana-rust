use assert_cmd::prelude::*;
use serde_json::Value;
use std::env::temp_dir;
use std::process::{Command, Stdio};

fn oocana_cmd() -> Command {
    let mut cmd = Command::cargo_bin("oocana").unwrap();
    cmd.stdin(Stdio::null());
    cmd
}

fn query_to_json(args: &[&str], tmp_name: &str) -> Value {
    let tmp_file = temp_dir().join(tmp_name);
    let tmp_path = tmp_file.to_str().unwrap();

    let mut full_args: Vec<&str> = args.to_vec();
    full_args.extend(["--output", tmp_path]);

    oocana_cmd().args(&full_args).assert().success();

    let content = std::fs::read_to_string(&tmp_file)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", tmp_path, e));

    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Invalid JSON in {}: {}\nContent: {}", tmp_path, e, content))
}

#[test]
fn query_services() {
    oocana_cmd()
        .args(["query", "service", "examples/service/flow.oo.yaml"])
        .assert()
        .success();
}

#[test]
fn query_upstream() {
    oocana_cmd()
        .args(["query", "upstream", "--nodes", "block-6", "examples/base"])
        .assert()
        .success();
}

#[test]
fn query_inputs() {
    let result = query_to_json(
        &["query", "inputs", "examples/base/pkg_a/blocks/blk-b"],
        "oocana_test_query_inputs.json",
    );

    assert!(result.is_object(), "Expected object, got: {}", result);
    assert!(
        result.get("my_count").is_some(),
        "Expected 'my_count' key in output, got: {}",
        result
    );
}

#[test]
fn query_nodes_inputs() {
    let result = query_to_json(
        &["query", "nodes-inputs", "examples/input"],
        "oocana_test_query_nodes_inputs.json",
    );

    let map = result.as_object().expect("Expected object at root");
    assert!(map.contains_key("block-1"), "Missing 'block-1' in output");

    for (node, inputs) in map {
        assert_node_inputs(node, inputs);
    }
}

fn assert_node_inputs(node: &str, inputs: &Value) {
    let arr = inputs
        .as_array()
        .unwrap_or_else(|| panic!("Node '{}': expected array, got {}", node, inputs));

    assert_eq!(
        arr.len(),
        2,
        "Node '{}': expected 2 inputs, got {}",
        node,
        arr.len()
    );

    for item in arr {
        assert_input_item(node, item);
    }
}

fn assert_input_item(node: &str, item: &Value) {
    let obj = item
        .as_object()
        .unwrap_or_else(|| panic!("Node '{}': input should be object, got {}", node, item));

    let handle = obj
        .get("handle")
        .and_then(|h| h.as_str())
        .unwrap_or_else(|| panic!("Node '{}': missing 'handle' field in {}", node, item));

    match handle {
        "in" => {
            assert!(
                obj.get("value").is_some_and(|v| v.is_number()),
                "Node '{}': handle 'in' should have number value, got {}",
                node,
                item
            );
        }
        "my_count" => {
            assert!(
                obj.get("value").is_none(),
                "Node '{}': handle 'my_count' should have no value, got {}",
                node,
                item
            );
        }
        _ => panic!("Node '{}': unexpected handle '{}'", node, handle),
    }
}
