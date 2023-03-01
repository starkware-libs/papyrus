use goose::goose::Scenario;
use goose::scenario;

use crate::transactions::*;

pub fn general_request() -> Scenario {
    let mut scenario= scenario!("general_request");
    let trans_and_weights=vec![
        (get_block_with_transaction_hashes_by_number(), 1),
        (get_block_with_transaction_hashes_by_hash(),   1),
        (block_number(),1),
        (block_hash_and_number(),1 ),
        (chain_id(), 1),
    ];
    for (transaction, weight) in trans_and_weights.into_iter(){
        scenario=scenario.register_transaction(transaction.set_weight(weight).unwrap());
    }
    scenario
}
