// This code is inspired by the pathfinder load test.
use std::env;

use goose::config::{GooseConfiguration, GooseDefault, GooseDefaultType};
use goose::goose::{GooseUser, Scenario, Transaction, TransactionError, TransactionResult};
use goose::{scenario, transaction, GooseAttack, GooseError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;

type MethodResult<T> = Result<T, TransactionError>;

async fn post_jsonrpc_request<T: DeserializeOwned + std::fmt::Debug + std::clone::Clone>(
    user: &mut GooseUser,
    method: &str,
    params: serde_json::Value,
) -> MethodResult<T> {
    let request = jsonrpc_request(method, params);
    let response = user.post_json("", &request).await?.response?;
    #[derive(Deserialize, Debug)]
    struct TransactionReceiptResponse<T>
    where
        T: Clone,
    {
        result: T,
    }
    let response = response.json::<TransactionReceiptResponse<T>>().await;
    Ok(response?.result)
}

fn jsonrpc_request(method: &str, params: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
}
pub async fn task_get_block_with_tx_hashes(user: &mut GooseUser) -> TransactionResult {
    for i in 10..20 {
        let val: serde_json::Value = get_block_with_tx_hashes_by_number(user, i).await?;
        let val_v = &val["block_hash"];
        let val_h: serde_json::Value =
            get_block_with_tx_hashes_by_hash(user, val_v.as_str().unwrap()).await?;
        assert!(val == val_h);
    }

    Ok(())
}

pub async fn get_block_with_tx_hashes_by_number<
    T: DeserializeOwned + std::fmt::Debug + std::clone::Clone,
>(
    user: &mut GooseUser,
    number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getBlockWithTxHashes", json!([{ "block_number": number }]))
        .await
}

pub async fn get_block_with_tx_hashes_by_hash<
    T: DeserializeOwned + std::fmt::Debug + std::clone::Clone,
>(
    user: &mut GooseUser,
    hash_str: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getBlockWithTxHashes", json!([{ "block_hash": hash_str }]))
        .await
}

pub async fn loadtest_get_block_with_tx_hashes_by_hash(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_block_with_tx_hashes_by_hash(
        user,
        "0x21d419ca19117002954f58f1fdf879791973107ccdb2004c5d635411bb90c0e",
    )
    .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), GooseError> {
    let node_endpoint = match env::var("NODE_ENDPOINT") {
        Ok(val) => val,
        _ => "http://127.0.0.1:8080".to_string(),
    };

    let mut config = GooseConfiguration::default();
    config.run_time = "5s".to_string();
    GooseAttack::initialize_with_config(config)?
        .register_scenario(
            scenario!("block_by_number")
                .register_transaction(transaction!(task_get_block_with_tx_hashes)),
        )
        .register_scenario(
            scenario!("block_by_hash")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_hash)),
        )
        .set_default(GooseDefault::Host, node_endpoint.as_str())?
        .execute()
        .await?;

    Ok(())
}
