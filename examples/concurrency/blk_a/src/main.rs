#[tokio::main]
async fn main() {
    let (sdk, event_loop) = oocana_sdk::connect().await;

    let mut result = 1;

    println!("Result of task {} is {}", &sdk.job_id, &result);

    sdk.output(&oocana_sdk::json!(result), "my_output");

    result += 3;
    sdk.output(&oocana_sdk::json!(result), "my_output");

    result += 3;

    sdk.finish(
        Some(
            vec![("my_output".to_string(), result.into())]
                .into_iter()
                .collect(),
        ),
        None,
    );

    event_loop.wait().await;
}
