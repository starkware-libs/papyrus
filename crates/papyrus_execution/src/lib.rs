#[cfg(test)]
#[path = "execution_test.rs"]
mod execution_test;

use blockifier::execution::entry_point::Retdata;
use blockifier::execution::errors::EntryPointExecutionError;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, EntryPointSelector};
use starknet_api::transaction::Calldata;

pub type ExecutionResult<T> = Result<T, ExecutionError>;

#[derive(thiserror::Error, Debug)]
pub enum ExecutionError {
    #[error(transparent)]
    BlockifierError(#[from] EntryPointExecutionError),
}

/// Executes a StarkNet call and returns the retdata.
// TODO(yair): Consider adding Retdata to StarkNetApi.
pub fn execute_call(
    block_number: &BlockNumber,
    contract_address: &ContractAddress,
    entry_point_selector: &EntryPointSelector,
    calldata: Calldata,
) -> ExecutionResult<Retdata> {
    todo!()
}
