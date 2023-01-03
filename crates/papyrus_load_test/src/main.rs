// This code is inspired by the pathfinder load test.

use goose::goose::{GooseUser, Scenario, Transaction, TransactionError, TransactionResult};
use goose::{scenario, transaction, GooseAttack, GooseError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;

type MethodResult<T> = Result<T, Box<TransactionError>>;

async fn post_jsonrpc_request<T: DeserializeOwned>(
    user: &mut GooseUser,
    method: &str,
    params: serde_json::Value,
) -> MethodResult<T> {
    let request = jsonrpc_request(method, params);
    let response = user.post_json("", &request).await?.response.map_err(|e| Box::new(e.into()))?;
    #[derive(Deserialize)]
    struct TransactionReceiptResponse<T> {
        result: T,
    }
    let response: TransactionReceiptResponse<T> =
        response.json().await.map_err(|e| Box::new(e.into()))?;

    Ok(response.result)
}

fn jsonrpc_request(method: &str, params: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}

async fn loadtest_get_block_with_tx_hashes_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_block_with_tx_hashes_by_number(user, 1).await?;
    Ok(())
}

pub async fn get_block_with_tx_hashes_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getBlockWithTxHashes", json!([{ "block_number": number }]))
        .await
}

async fn loadtest_get_block_with_tx_hashes_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_with_tx_hashes_by_hash(
        user,
        "0x21d419ca19117002954f58f1fdf879791973107ccdb2004c5d635411bb90c0e",
    )
    .await?;
    Ok(())
}

pub async fn get_block_with_tx_hashes_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    hash_str: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getBlockWithTxHashes", json!([{ "block_hash": hash_str }]))
        .await
}

#[tokio::main]
async fn main() -> Result<(), GooseError> {
    GooseAttack::initialize()?
        .register_scenario(
            scenario!("block_by_number")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_number)),
        )
        .register_scenario(
            scenario!("block_by_hash")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_hash)),
        )
        .execute()
        .await?;
    Ok(())
}
