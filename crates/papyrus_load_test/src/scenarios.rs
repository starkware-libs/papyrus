use goose::goose::Scenario;
use goose::scenario;

use crate::transactions;

pub fn block_by_number() -> Scenario {
    scenario!("block_by_number")
        .register_transaction(transactions::get_block_with_tx_hashes_by_number())
}

pub fn block_by_hash() -> Scenario {
    scenario!("block_by_hash")
        .register_transaction(transactions::get_block_with_tx_hashes_by_hash())
}
