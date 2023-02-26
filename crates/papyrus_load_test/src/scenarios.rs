use goose::goose::Scenario;
use goose::scenario;

use crate::transactions;
pub type ScenariosResult = Result<Scenario, ScenariosError>;

#[derive(thiserror::Error, Debug)]
pub enum ScenariosError {
    #[error(transparent)]
    CreateTransaction(#[from] transactions::TransactionsError),
    #[error(transparent)]
    Goose(#[from] goose::GooseError),
}

pub fn general_request() -> ScenariosResult {
    Ok(scenario!("general_request")
        .register_transaction(transactions::get_block_with_tx_hashes_by_number()?.set_weight(1)?)
        .register_transaction(transactions::get_block_with_tx_hashes_by_hash()?.set_weight(1)?))
}
