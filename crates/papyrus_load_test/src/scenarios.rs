use goose::goose::Scenario;
use goose::scenario;

use crate::{
    transactions as txs,
    BLOCK_HASH_AND_NUMBER_WEIGHT,
    BLOCK_NUMBER_WEIGHT,
    CHAIN_ID_WEIGHT,
    GET_BLOCK_TRANSACTION_COUNT_BY_HASH_WEIGHT,
    GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER_WEIGHT,
    GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_HASH_WEIGHT,
    GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_NUMBER_WEIGHT,
    GET_BLOCK_WITH_TRANSACTION_HASHES_BY_HASH_WEIGHT,
    GET_BLOCK_WITH_TRANSACTION_HASHES_BY_NUMBER_WEIGHT,
    GET_CLASS_AT_BY_HASH_WEIGHT,
    GET_CLASS_AT_BY_NUMBER_WEIGHT,
    GET_CLASS_BY_HASH_WEIGHT,
    GET_CLASS_BY_NUMBER_WEIGHT,
    GET_CLASS_HASH_AT_BY_HASH_WEIGHT,
    GET_CLASS_HASH_AT_BY_NUMBER_WEIGHT,
    GET_EVENTS_WITHOUT_ADDRESS_WEIGHT,
    GET_EVENTS_WITH_ADDRESS_WEIGHT,
    GET_NONCE_BY_HASH_WEIGHT,
    GET_NONCE_BY_NUMBER_WEIGHT,
    GET_STATE_UPDATE_BY_HASH_WEIGHT,
    GET_STATE_UPDATE_BY_NUMBER_WEIGHT,
    GET_STORAGE_AT_BY_HASH_WEIGHT,
    GET_STORAGE_AT_BY_NUMBER_WEIGHT,
    GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_HASH_WEIGHT,
    GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_NUMBER_WEIGHT,
    GET_TRANSACTION_BY_HASH_WEIGHT,
    GET_TRANSACTION_RECEIPT_WEIGHT,
};

pub fn general_request() -> Scenario {
    let mut scenario = scenario!("general_request");
    let trans_and_weights = vec![
        (txs::block_hash_and_number(), BLOCK_HASH_AND_NUMBER_WEIGHT),
        (txs::block_number(), BLOCK_NUMBER_WEIGHT),
        (txs::chain_id(), CHAIN_ID_WEIGHT),
        (txs::get_block_transaction_count_by_hash(), GET_BLOCK_TRANSACTION_COUNT_BY_HASH_WEIGHT),
        (
            txs::get_block_transaction_count_by_number(),
            GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER_WEIGHT,
        ),
        (
            txs::get_block_with_full_transactions_by_hash(),
            GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_HASH_WEIGHT,
        ),
        (
            txs::get_block_with_full_transactions_by_number(),
            GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_NUMBER_WEIGHT,
        ),
        (
            txs::get_block_with_transaction_hashes_by_hash(),
            GET_BLOCK_WITH_TRANSACTION_HASHES_BY_HASH_WEIGHT,
        ),
        (
            txs::get_block_with_transaction_hashes_by_number(),
            GET_BLOCK_WITH_TRANSACTION_HASHES_BY_NUMBER_WEIGHT,
        ),
        (txs::get_class_at_by_hash(), GET_CLASS_AT_BY_HASH_WEIGHT),
        (txs::get_class_at_by_number(), GET_CLASS_AT_BY_NUMBER_WEIGHT),
        (txs::get_class_by_hash(), GET_CLASS_BY_HASH_WEIGHT),
        (txs::get_class_by_number(), GET_CLASS_BY_NUMBER_WEIGHT),
        (txs::get_class_hash_at_by_hash(), GET_CLASS_HASH_AT_BY_HASH_WEIGHT),
        (txs::get_class_hash_at_by_number(), GET_CLASS_HASH_AT_BY_NUMBER_WEIGHT),
        (txs::get_events_without_address(), GET_EVENTS_WITHOUT_ADDRESS_WEIGHT),
        (txs::get_events_with_address(), GET_EVENTS_WITH_ADDRESS_WEIGHT),
        (txs::get_nonce_by_hash(), GET_NONCE_BY_HASH_WEIGHT),
        (txs::get_nonce_by_number(), GET_NONCE_BY_NUMBER_WEIGHT),
        (txs::get_state_update_by_hash(), GET_STATE_UPDATE_BY_HASH_WEIGHT),
        (txs::get_state_update_by_number(), GET_STATE_UPDATE_BY_NUMBER_WEIGHT),
        (txs::get_storage_at_by_hash(), GET_STORAGE_AT_BY_HASH_WEIGHT),
        (txs::get_storage_at_by_number(), GET_STORAGE_AT_BY_NUMBER_WEIGHT),
        (
            txs::get_transaction_by_block_id_and_index_by_hash(),
            GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_HASH_WEIGHT,
        ),
        (
            txs::get_transaction_by_block_id_and_index_by_number(),
            GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_NUMBER_WEIGHT,
        ),
        (txs::get_transaction_by_hash(), GET_TRANSACTION_BY_HASH_WEIGHT),
        (txs::get_transaction_receipt(), GET_TRANSACTION_RECEIPT_WEIGHT),
    ];
    for (transaction, weight) in trans_and_weights.into_iter() {
        scenario = scenario.register_transaction(transaction.set_weight(weight).unwrap());
    }
    scenario
}
