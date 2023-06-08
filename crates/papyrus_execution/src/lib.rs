#[cfg(test)]
#[path = "execution_test.rs"]
mod execution_test;

mod execution_utils;
mod state_reader;

use blockifier::execution::entry_point::{CallEntryPoint, CallType, Retdata};
use blockifier::execution::errors::{EntryPointExecutionError, PreExecutionError};
use blockifier::state::cached_state::CachedState;
use execution_utils::ExecutionUtilsError;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::state::StateNumber;
use starknet_api::transaction::Calldata;
use state_reader::ExecutionStateReader;

pub type ExecutionResult<T> = Result<T, ExecutionError>;

#[derive(thiserror::Error, Debug)]
pub enum ExecutionError {
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    ExecutionUtilsError(#[from] ExecutionUtilsError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(
        "The contract was not found. state_number: {state_number:?}, contract_address: \
         {contract_address:?}"
    )]
    ContractNotFound { contract_address: ContractAddress, state_number: StateNumber },
    #[error("No blocks at state 0.")]
    NoData,
    #[error(
        "The node is not synced. state_number: {state_number:?}, compiled_class_marker: \
         {compiled_class_marker:?}"
    )]
    NotSynced { state_number: StateNumber, compiled_class_marker: BlockNumber },
    #[error(transparent)]
    PreExecutionError(#[from] PreExecutionError),
}

/// Executes a StarkNet call and returns the retdata.
// TODO(yair): Consider adding Retdata to StarkNetApi.
pub fn execute_call(
    storage_reader: StorageReader,
    state_number: StateNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
) -> ExecutionResult<Retdata> {
    let txn = storage_reader.begin_ro_txn()?;

    // Verify the node is synced.
    let synced_up_to = StateNumber::right_before_block(txn.get_compiled_class_marker()?);
    if state_number > synced_up_to {
        return Err(ExecutionError::NotSynced {
            state_number,
            compiled_class_marker: synced_up_to.block_after(),
        });
    }

    let class_hash =
        txn.get_state_reader()?.get_class_hash_at(state_number, contract_address)?.ok_or(
            ExecutionError::ContractNotFound { contract_address: *contract_address, state_number },
        )?;

    let call_entry_point = CallEntryPoint {
        class_hash: Some(class_hash),
        code_address: None,
        entry_point_type: EntryPointType::External,
        entry_point_selector,
        calldata,
        storage_address: *contract_address,
        caller_address: ContractAddress::default(),
        call_type: CallType::Call,
    };

    let mut execution_state_reader =
        CachedState::new(ExecutionStateReader { txn: &txn, state_number });

    let res = call_entry_point.execute_directly(&mut execution_state_reader)?;
    Ok(res.execution.retdata)
}
