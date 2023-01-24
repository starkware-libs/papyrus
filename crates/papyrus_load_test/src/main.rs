// This code is inspired by the pathfinder load test.

use std::env;
use std::fs::File;

use goose::goose::{GooseUser, Scenario, Transaction, TransactionError, TransactionResult};
use goose::{scenario, transaction, util, GooseAttack};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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

/////////////////////////////////////////////////////////////////////
// load tests
/////////////////////////////////////////////////////////////////////
async fn loadtest_get_block_with_tx_hashes_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_block_w_tx_hashes_by_number(user, 1).await?;
    Ok(())
}

async fn loadtest_get_block_with_tx_hashes_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_w_tx_hashes_by_hash(
        user,
        "0x1d997fd79d81bb4c30c78d7cb32fb8a59112eeb86347446235cead6194aed07",
    )
    .await?;
    Ok(())
}

/////////////////////////////////////////////////////////////////////
// functions for gateways requests
/////////////////////////////////////////////////////////////////////

// block_number
pub async fn get_block_number<T: DeserializeOwned>(user: &mut GooseUser) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_blockNumber", json!([])).await
}

// block_hash_and_number
pub async fn get_block_hash_and_number<T: DeserializeOwned>(
    user: &mut GooseUser,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_blockHashAndNumber", json!([])).await
}

// get_block_w_transaction_hashes
pub async fn get_block_w_tx_hashes_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockWithTxHashes",
        json!([{ "block_number": block_number }]),
    )
    .await
}
pub async fn get_block_w_tx_hashes_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockWithTxHashes",
        json!([{ "block_hash": block_hash }]),
    )
    .await
}

// get_block_w_full_transactions
pub async fn get_block_w_full_transactions_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockWithTxs",
        json!([{ "block_number": block_number }]),
    )
    .await
}

pub async fn get_block_w_full_transactions_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getBlockWithTxs", json!([{ "block_hash": block_hash }]))
        .await
}

// get_storage_at
pub async fn get_storage_at_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    contract_address: &str,
    key: &str,
    block_number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getStorageAt",
        json!([{ "contract_address": contract_address, "key": key, "block_number": block_number }]),
    )
    .await
}

pub async fn get_storage_at_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    contract_address: &str,
    key: &str,
    block_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getStorageAt",
        json!([{ "contract_address": contract_address, "key": key, "block_hash": block_hash }]),
    )
    .await
}

// get_transaction_by_hash
pub async fn get_transaction_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    transaction_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getTransactionByHash",
        json!([{ "transaction_hash": transaction_hash }]),
    )
    .await
}

// get_transaction_by_block_id_and_index
pub async fn get_transaction_by_block_id_and_index_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
    index: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getTransactionByBlockIdAndIndex",
        json!([{"block_number": block_number, "index": index }]),
    )
    .await
}

pub async fn get_transaction_by_block_id_and_index_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
    index: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getTransactionByBlockIdAndIndex",
        json!([{ "block_hash": block_hash, "index": index }]),
    )
    .await
}

// get_block_transaction_count
pub async fn get_block_transaction_count_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockTransactionCount",
        json!([{ "block_number": block_number }]),
    )
    .await
}
pub async fn get_block_transaction_count_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockTransactionCount",
        json!([{ "block_hash": block_hash }]),
    )
    .await
}

// get_state_update
pub async fn get_state_update_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getStateUpdate", json!([{ "block_number": block_number }]))
        .await
}
pub async fn get_state_update_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getStateUpdate", json!([{ "block_hash": block_hash }]))
        .await
}

// get_transaction_receipt
pub async fn get_transaction_receipt<T: DeserializeOwned>(
    user: &mut GooseUser,
    transaction_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getTransactionReceipt",
        json!([{ "transaction_hash": transaction_hash }]),
    )
    .await
}

// get_class
pub async fn get_class_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
    class_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getClass",
        json!([{ "block_number": block_number, "class_hash": class_hash }]),
    )
    .await
}
pub async fn get_class_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
    class_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getClass",
        json!([{ "block_hash": block_hash, "class_hash": class_hash }]),
    )
    .await
}

// get_class_at
pub async fn get_class_at_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
    contract_address: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getClassAt",
        json!([{ "block_number": block_number, "contract_address": contract_address }]),
    )
    .await
}
pub async fn get_class_at_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
    contract_address: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getClassAt",
        json!([{ "block_hash": block_hash, "contract_address": contract_address }]),
    )
    .await
}

// get_class_hash_at
pub async fn get_class_hash_at_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
    contract_address: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getClassHashAt",
        json!([{ "block_number": block_number, "contract_address": contract_address }]),
    )
    .await
}
pub async fn get_class_hash_at_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
    contract_address: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getClassHashAt",
        json!([{ "block_hash": block_hash, "contract_address": contract_address }]),
    )
    .await
}

// get_nonce
pub async fn get_nonce_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
    contract_address: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getNonce",
        json!([{ "block_number": block_number, "contract_address": contract_address }]),
    )
    .await
}
pub async fn get_nonce_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
    contract_address: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getNonce",
        json!([{ "block_hash": block_hash, "contract_address": contract_address }]),
    )
    .await
}

// chain_id
pub async fn chain_id<T: DeserializeOwned>(user: &mut GooseUser) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_blockNumber", json!([])).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The OUTPUT_FILE env is expected to be a valid path in the os.
    // If exists, aggregated results will be written to that path in the following json format:
    // [
    //     {
    //         "name": <scenario name>,
    //         "units": "Milliseconds",
    //         "value": <scenario median time>,
    //     },
    // ]

    let output_file = match env::var("OUTPUT_FILE") {
        Ok(path) => Some(path),
        Err(_) => None,
    };

    let metrics = GooseAttack::initialize()?
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

    // Optionally write results to the given path.
    if let Some(path) = output_file {
        let file = File::create(path)?;
        let mut data: Vec<Entry> = vec![];
        for scenario in metrics.scenarios {
            let median = util::median(
                &scenario.times,
                scenario.counter,
                scenario.min_time,
                scenario.max_time,
            );
            data.push(Entry {
                name: scenario.name,
                units: "Milliseconds".to_string(),
                value: median,
            });
        }
        serde_json::to_writer(file, &data)?
    }

    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Entry {
    name: String,
    units: String, // "Milliseconds"
    value: usize,
}
