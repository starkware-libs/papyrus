use goose::goose::Scenario;
use goose::scenario;

use crate::transactions;

pub fn general_request() -> Scenario {
    scenario!("general_request")
        .register_transaction(
            transactions::get_block_with_tx_hashes_by_number().set_weight(1).unwrap(),
        )
        .register_transaction(
            transactions::get_block_with_tx_hashes_by_hash().set_weight(1).unwrap(),
        )
}

pub fn serial_get_block() -> Scenario {
    scenario!("general_request")
        .register_transaction(transactions::serial_get_block(1000))
}
