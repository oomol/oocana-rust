use vocana_sdk::{json, VocanaSDK};

fn main() {
    let mut sdk = VocanaSDK::new();
    sdk.connect().unwrap();

    let count: i64 = sdk
        .block_input
        .as_ref()
        .and_then(|block_input| block_input.get("count")?.as_i64())
        .unwrap_or(0);

    let output = count + 1;

    println!(
        "Result of block task {} is {}",
        &sdk.block_meta.block_task_id, &output
    );

    sdk.output("out", true, json!(output)).unwrap();
}
