#[tokio::main]
async fn main() {
    let (sdk, event_loop) = oocana_sdk::connect().await;

    let count: i64 = sdk
        .inputs
        .as_ref()
        .and_then(|inputs| inputs.get("in")?.as_i64())
        .unwrap_or(0);

    let result = count;

    sdk.output(&oocana_sdk::json!(result), "out", true);

    event_loop.wait().await;
}
