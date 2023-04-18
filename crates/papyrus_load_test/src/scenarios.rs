use goose::goose::Scenario;
use goose::scenario;

use crate::transactions as txs;

pub fn general_request() -> Scenario {
    let mut scenario = scenario!("general_request");
    let trans_and_weights = vec![
        (txs::get_block_with_transaction_hashes_by_number(), 1),
        (txs::get_block_with_transaction_hashes_by_hash(), 1),
        (txs::get_block_with_full_transactions_by_number(), 1),
        (txs::get_block_with_full_transactions_by_hash(), 1),
        (txs::get_block_transaction_count_by_number(), 1),
        (txs::get_block_transaction_count_by_hash(), 1),
        (txs::get_state_update_by_number(), 1),
        (txs::get_state_update_by_hash(), 1),
        (txs::block_number(), 1),
        (txs::block_hash_and_number(), 1),
        (txs::chain_id(), 1),
        (txs::get_transaction_by_block_id_and_index_by_hash(), 1),
        (txs::get_transaction_by_hash(), 1),
        (txs::get_transaction_receipt(), 1),
        (txs::get_transaction_by_block_id_and_index_by_number(), 1),
        (txs::get_class_at_by_number(), 1),
        (txs::get_class_at_by_hash(), 1),
        (txs::get_class_hash_at_by_number(), 1),
        (txs::get_class_hash_at_by_hash(), 1),
        (txs::get_nonce_by_number(), 1),
        (txs::get_nonce_by_hash(), 1),
        (txs::get_storage_at_by_number(), 1),
        (txs::get_storage_at_by_hash(), 1),
        (txs::get_events_with_address(), 1),
        (txs::get_events_without_address(), 1),
    ];
    for (transaction, weight) in trans_and_weights.into_iter() {
        scenario = scenario.register_transaction(transaction.set_weight(weight).unwrap());
    }
    scenario
}
