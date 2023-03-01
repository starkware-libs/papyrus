use goose::goose::Scenario;
use goose::scenario;

use crate::transactions as txs;

pub fn general_request() -> Scenario {
    let mut scenario = scenario!("general_request");
    let trans_and_weights = vec![
        (txs::get_block_with_transaction_hashes_by_number(), 1),
        (txs::get_block_with_transaction_hashes_by_hash(), 1),
        (txs::block_number(), 1),
        (txs::block_hash_and_number(), 1),
        (txs::chain_id(), 1),
        (txs::get_transaction_by_block_id_and_index_by_hash(), 1),
    ];
    for (transaction, weight) in trans_and_weights.into_iter() {
        scenario = scenario.register_transaction(transaction.set_weight(weight).unwrap());
    }
    scenario
}
