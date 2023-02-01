use goose::goose::{GooseUser, TransactionResult};

use crate::gateway_functions::*;

const BLOCK_HASH: &str = "0x47c3637b57c2b079b93c61539950c17e868a28f46cdef28f88521067f21e943";
const BLOCK_NUMBER: u64 = 0;
const TRANSACTION_HASH: &str = "0xce54bbc5647e1c1ea4276c01a708523f740db0ff5474c77734f73beec2624";
const CLASS_HASH: &str = "0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8";
const TRANSACTION_INDEX: usize = 0;
const CONTRACT_ADDRESS: &str = "0x20cfa74ee3564b4cd5435cdace0f9c4d43b939620e4a0bb5076105df0a626c6";
const KEY: &str = "0x20cfa74ee3564b4cd5435cdace0f9c4d43b939620e4a0bb5076105df0a626c6";

// block_number
pub async fn loadtest_block_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = block_number(user).await?;
    Ok(())
}

// block_hash_and_number
pub async fn loadtest_block_hash_and_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = block_hash_and_number(user).await?;
    Ok(())
}

// get_block_w_transaction_hashes
pub async fn loadtest_get_block_with_tx_hashes_by_number(
    user: &mut GooseUser,
) -> TransactionResult {
    let _: serde_json::Value = get_block_with_tx_hashes_by_number(user, BLOCK_NUMBER).await?;
    Ok(())
}

pub async fn loadtest_get_block_with_tx_hashes_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_with_tx_hashes_by_hash(user, BLOCK_HASH).await?;
    Ok(())
}

// get_block_w_full_transactions
pub async fn loadtest_get_block_with_full_transactions_by_number(
    user: &mut GooseUser,
) -> TransactionResult {
    let _: serde_json::Value =
        get_block_with_full_transactions_by_number(user, BLOCK_NUMBER).await?;
    Ok(())
}

pub async fn loadtest_get_block_with_full_transactions_by_hash(
    user: &mut GooseUser,
) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_with_full_transactions_by_hash(user, BLOCK_HASH).await?;
    Ok(())
}

// get_storage_at
pub async fn loadtest_get_storage_at_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value =
        get_storage_at_by_number(user, CONTRACT_ADDRESS, KEY, BLOCK_NUMBER).await?;
    Ok(())
}

pub async fn loadtest_get_storage_at_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value =
        get_storage_at_by_hash(user, CONTRACT_ADDRESS, KEY, BLOCK_HASH).await?;
    Ok(())
}

// get_transaction_by_hash
pub async fn loadtest_get_transaction_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_transaction_by_hash(user, TRANSACTION_HASH).await?;
    Ok(())
}

// get_transaction_by_block_id_and_index
pub async fn loadtest_get_transaction_by_block_id_and_index_by_number(
    user: &mut GooseUser,
) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value =
        get_transaction_by_block_id_and_index_by_number(user, BLOCK_NUMBER, TRANSACTION_INDEX)
            .await?;
    Ok(())
}

pub async fn loadtest_get_transaction_by_block_id_and_index_by_hash(
    user: &mut GooseUser,
) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value =
        get_transaction_by_block_id_and_index_by_hash(user, BLOCK_HASH, TRANSACTION_INDEX).await?;
    Ok(())
}

// get_block_transaction_count
pub async fn loadtest_get_block_transaction_count_by_number(
    user: &mut GooseUser,
) -> TransactionResult {
    let _: serde_json::Value = get_block_transaction_count_by_number(user, BLOCK_NUMBER).await?;
    Ok(())
}

pub async fn loadtest_get_block_transaction_count_by_hash(
    user: &mut GooseUser,
) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_transaction_count_by_hash(user, BLOCK_HASH).await?;
    Ok(())
}

// get_state_update
pub async fn loadtest_get_state_update_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_state_update_by_number(user, BLOCK_NUMBER).await?;
    Ok(())
}

pub async fn loadtest_get_state_update_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_state_update_by_hash(user, BLOCK_HASH).await?;
    Ok(())
}

// get_transaction_receipt
pub async fn loadtest_get_transaction_receipt(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_transaction_receipt(user, TRANSACTION_HASH).await?;
    Ok(())
}

// get_class
pub async fn loadtest_get_class_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_class_by_number(user, BLOCK_NUMBER, CLASS_HASH).await?;
    Ok(())
}

pub async fn loadtest_get_class_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_class_by_hash(user, BLOCK_HASH, CLASS_HASH).await?;
    Ok(())
}

// get_class_at
pub async fn loadtest_get_class_at_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_class_at_by_number(user, BLOCK_NUMBER, CONTRACT_ADDRESS).await?;
    Ok(())
}

pub async fn loadtest_get_class_at_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_class_at_by_hash(user, BLOCK_HASH, CONTRACT_ADDRESS).await?;
    Ok(())
}

// get_class_hash_at
pub async fn loadtest_get_class_hash_at_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value =
        get_class_hash_at_by_number(user, BLOCK_NUMBER, CONTRACT_ADDRESS).await?;
    Ok(())
}

pub async fn loadtest_get_class_hash_at_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value =
        get_class_hash_at_by_hash(user, BLOCK_HASH, CONTRACT_ADDRESS).await?;
    Ok(())
}

// get_nonce
pub async fn loadtest_get_nonce_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_nonce_by_number(user, BLOCK_NUMBER, CONTRACT_ADDRESS).await?;
    Ok(())
}

pub async fn loadtest_get_nonce_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_nonce_by_hash(user, BLOCK_HASH, CONTRACT_ADDRESS).await?;
    Ok(())
}

// chain_id
pub async fn loadtest_chain_id(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = chain_id(user).await?;
    Ok(())
}

// ADD get_events !!!!!!!
