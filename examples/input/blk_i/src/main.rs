#[tokio::main]
async fn main() {
    let (sdk, event_loop) = oocana_sdk::connect().await;

    let count: i64 = sdk
        .inputs
        .as_ref()
        .and_then(|inputs| inputs.get("my_count")?.as_i64())
        .unwrap_or(0);

    let result = count + 1;

    println!("Result of task {} is {}", &sdk.job_id, &result);

    sdk.output(&oocana_sdk::json!(result), "my_output", true);

    event_loop.wait().await;
}
