#[tokio::main]
async fn main() {
    let (sdk, event_loop) = oocana_sdk::connect().await;

    let mut result = 1;

    println!("Result of task {} is {}", &sdk.job_id, &result);

    sdk.output(&oocana_sdk::json!(result), "my_output", false);

    result += 3;
    sdk.output(&oocana_sdk::json!(result), "my_output", false);

    result += 3;
    sdk.output(&oocana_sdk::json!(result), "my_output", true);

    event_loop.wait().await;
}
