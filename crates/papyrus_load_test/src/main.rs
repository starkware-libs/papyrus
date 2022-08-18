use goose::goose::{GooseUser, Scenario, Transaction, TransactionError, TransactionResult};
use goose::{scenario, transaction, GooseAttack, GooseError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;

type MethodResult<T> = Result<T, TransactionError>;

// Taken from pathfinder.
async fn post_jsonrpc_request<T: DeserializeOwned>(
    user: &mut GooseUser,
    method: &str,
    params: serde_json::Value,
) -> MethodResult<T> {
    let request = jsonrpc_request(method, params);
    let response = user.post_json("", &request).await?.response?;
    #[derive(Deserialize)]
    struct TransactionReceiptResponse<T> {
        result: T,
    }
    let response: TransactionReceiptResponse<T> = response.json().await?;

    Ok(response.result)
}

// Taken from pathfinder.
fn jsonrpc_request(method: &str, params: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}

async fn loadtest_getBlockWithTxHashes_by_number(user: &mut GooseUser) -> TransactionResult {
    post_jsonrpc_request(
        user,
        "starknet_getBlockWithTxHashes",
        json!({ "block_id": { "block_number": 1 } }),
    )
    .await
}

#[tokio::main]
async fn main() -> Result<(), GooseError> {
    GooseAttack::initialize()?
        .register_scenario(
            scenario!("block_by_number")
                .register_transaction(transaction!(loadtest_getBlockWithTxHashes_by_number)),
        )
        .execute()
        .await?;
    Ok(())
}
