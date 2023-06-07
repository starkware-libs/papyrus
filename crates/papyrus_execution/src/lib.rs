#[cfg(test)]
#[path = "execution_test.rs"]
mod execution_test;

mod execution_utils;
mod state_reader;

use std::collections::HashMap;

use blockifier::abi::constants::INITIAL_GAS_COST;
use blockifier::block_context::BlockContext;
use blockifier::execution::entry_point::{
    CallEntryPoint, CallExecution, CallType, EntryPointExecutionContext, ExecutionResources,
};
use blockifier::execution::errors::{EntryPointExecutionError, PreExecutionError};
use blockifier::state::cached_state::CachedState;
use blockifier::transaction::objects::AccountTransactionContext;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::state::StateNumber;
use starknet_api::transaction::Calldata;
use state_reader::ExecutionStateReader;

pub type ExecutionResult<T> = Result<T, ExecutionError>;

const EXECUTION_GAS_PRICE: u128 = 100 * u128::pow(10, 9); // Given in units of wei.
const INVOKE_TX_MAX_N_STEPS: u32 = 1_000_000;
const VALIDATE_TX_MAX_N_STEPS: u32 = 1_000_000;

#[derive(thiserror::Error, Debug)]
pub enum ExecutionError {
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
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

/// Executes a StarkNet call and returns the execution result.
pub fn execute_call(
    txn: &StorageTxn<'_, RO>,
    chain_id: &ChainId,
    state_number: StateNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
) -> ExecutionResult<CallExecution> {
    // Verify the node is synced.
    let synced_up_to = StateNumber::right_before_block(txn.get_compiled_class_marker()?);
    if state_number > synced_up_to {
        return Err(ExecutionError::NotSynced {
            state_number,
            compiled_class_marker: synced_up_to.block_after(),
        });
    }

    let call_entry_point = CallEntryPoint {
        class_hash: None,
        code_address: Some(*contract_address),
        entry_point_type: EntryPointType::External,
        entry_point_selector,
        calldata,
        storage_address: *contract_address,
        caller_address: ContractAddress::default(),
        call_type: CallType::Call,
        // todo(yair): Check if this is the correct value.
        initial_gas: INITIAL_GAS_COST.into(),
    };

    let mut execution_state_reader = CachedState::new(ExecutionStateReader { txn, state_number });
    let block_number = state_number.block_after().prev().expect("Should have a block.");
    let block_context = BlockContext {
        chain_id: chain_id.clone(),
        block_number,
        block_timestamp: txn
            .get_block_header(block_number)?
            .expect("Should have a block header.")
            .timestamp,
        sequencer_address: ContractAddress::default(),
        fee_token_address: ContractAddress::default(),
        vm_resource_fee_cost: HashMap::default(),
        gas_price: EXECUTION_GAS_PRICE,
        invoke_tx_max_n_steps: INVOKE_TX_MAX_N_STEPS,
        validate_max_n_steps: VALIDATE_TX_MAX_N_STEPS,
    };
    let mut context = EntryPointExecutionContext::new(
        block_context,
        AccountTransactionContext::default(),
        INVOKE_TX_MAX_N_STEPS,
    );

    let res = call_entry_point.execute(
        &mut execution_state_reader,
        &mut ExecutionResources::default(),
        &mut context,
    )?;

    Ok(res.execution)
}
