use goose::goose::{GooseUser, TransactionError};
use serde::de::DeserializeOwned;
use serde_json::json;
type MethodResult<T> = Result<T, Box<TransactionError>>;
use crate::post_jsonrpc_request;

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
pub async fn get_block_with_tx_hashes_by_number<T: DeserializeOwned>(
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
pub async fn get_block_with_tx_hashes_by_hash<T: DeserializeOwned>(
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
pub async fn get_block_with_full_transactions_by_number<T: DeserializeOwned>(
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

pub async fn get_block_with_full_transactions_by_hash<T: DeserializeOwned>(
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
    index: usize,
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
    index: usize,
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

// get_events
pub async fn get_events_with_just_chunk_size<T: DeserializeOwned>(
    user: &mut GooseUser,
    chunk_size: usize,
) -> MethodResult<T> {
    post_jsonrpc_request(user, "starknet_getEvents", json!([{ "filter": chunk_size }])).await
}
