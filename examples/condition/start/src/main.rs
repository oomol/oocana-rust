#[tokio::main]
async fn main() {
    let (sdk, event_loop) = oocana_sdk::connect().await;

    let mut result = 1;

    sdk.output(&oocana_sdk::json!(result), "output1", false);

    result += 3;
    sdk.output(&oocana_sdk::json!(result), "output2", true);

    event_loop.wait().await;
}
