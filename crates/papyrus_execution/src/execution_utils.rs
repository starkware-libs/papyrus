//! Utilities for executing contracts and transactions.
use std::fs::File;
use std::path::PathBuf;

// Expose the tool for creating entry point selectors from function names.
pub use blockifier::abi::abi_utils::selector_from_name;
use blockifier::execution::contract_class::{
    ContractClass as BlockifierContractClass,
    ContractClassV0,
    ContractClassV1,
};
use blockifier::state::cached_state::{CachedState, MutRefState};
use blockifier::state::state_api::{State, StateReader};
use blockifier::transaction::objects::TransactionExecutionInfo;
use cairo_vm::types::errors::program_errors::ProgramError;
use indexmap::IndexMap;
use papyrus_common::state::StorageEntry;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageResult, StorageTxn};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateNumber, StorageKey, ThinStateDiff};
use thiserror::Error;

use crate::objects::TransactionTrace;
use crate::state_reader::ExecutionStateReader;
use crate::{ExecutableTransactionInput, ExecutionConfigByBlock, ExecutionError, ExecutionResult};

// An error that can occur during the use of the execution utils.
#[derive(Debug, Error)]
pub(crate) enum ExecutionUtilsError {
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Casm table not fully synced")]
    CasmTableNotSynced,
}

/// Returns the execution config from the config file.
impl TryFrom<PathBuf> for ExecutionConfigByBlock {
    type Error = ExecutionError;

    fn try_from(execution_config_file: PathBuf) -> Result<Self, Self::Error> {
        let file = File::open(execution_config_file).map_err(ExecutionError::ConfigFileError)?;
        serde_json::from_reader(file).map_err(ExecutionError::ConfigSerdeError)
    }
}

pub(crate) fn get_contract_class(
    txn: &StorageTxn<'_, RO>,
    class_hash: &ClassHash,
    state_number: StateNumber,
) -> Result<Option<BlockifierContractClass>, ExecutionUtilsError> {
    match txn.get_state_reader()?.get_class_definition_block_number(class_hash)? {
        Some(block_number) if state_number.is_before(block_number) => return Ok(None),
        Some(_block_number) => {
            let Some(casm) = txn.get_casm(class_hash)? else {
                return Err(ExecutionUtilsError::CasmTableNotSynced);
            };
            return Ok(Some(BlockifierContractClass::V1(
                ContractClassV1::try_from(casm).map_err(ExecutionUtilsError::ProgramError)?,
            )));
        }
        None => {}
    };

    let Some(deprecated_class) =
        txn.get_state_reader()?.get_deprecated_class_definition_at(state_number, class_hash)?
    else {
        return Ok(None);
    };
    Ok(Some(BlockifierContractClass::V0(
        ContractClassV0::try_from(deprecated_class).map_err(ExecutionUtilsError::ProgramError)?,
    )))
}

/// Given an ExecutableTransactionInput, returns a function that will convert the corresponding
/// TransactionExecutionInfo into the right TransactionTrace variant.
pub fn get_trace_constructor(
    tx: &ExecutableTransactionInput,
) -> fn(TransactionExecutionInfo) -> ExecutionResult<TransactionTrace> {
    match tx {
        ExecutableTransactionInput::Invoke(_) => {
            |execution_info| Ok(TransactionTrace::Invoke(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV0(_, _) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV1(_, _) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV2(_, _) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeclareV3(_, _) => {
            |execution_info| Ok(TransactionTrace::Declare(execution_info.try_into()?))
        }
        ExecutableTransactionInput::DeployAccount(_) => {
            |execution_info| Ok(TransactionTrace::DeployAccount(execution_info.try_into()?))
        }
        ExecutableTransactionInput::L1Handler(_, _) => {
            |execution_info| Ok(TransactionTrace::L1Handler(execution_info.try_into()?))
        }
    }
}

/// Returns the state diff induced by a single transaction. If the transaction
/// is a deprecated Declare, the user is required to pass the class hash of the deprecated class as
/// it is not provided by the blockifier API.
pub fn induced_state_diff(
    transactional_state: &mut CachedState<MutRefState<'_, CachedState<ExecutionStateReader>>>,
    deprecated_declared_class_hash: Option<ClassHash>,
) -> ExecutionResult<ThinStateDiff> {
    let blockifier_state_diff = transactional_state.to_state_diff();

    // Determine which contracts were deployed and which were replaced by comparing their
    // previous class hash (default value suggests it didn't exist before).
    let mut deployed_contracts = IndexMap::new();
    let mut replaced_classes = IndexMap::new();
    let default_class_hash = ClassHash::default();
    for (address, class_hash) in blockifier_state_diff.address_to_class_hash.iter() {
        let prev_class_hash = transactional_state.state.get_class_hash_at(*address)?;
        if prev_class_hash == default_class_hash {
            deployed_contracts.insert(*address, *class_hash);
        } else {
            replaced_classes.insert(*address, *class_hash);
        }
    }
    Ok(ThinStateDiff {
        deployed_contracts,
        storage_diffs: blockifier_state_diff.storage_updates,
        declared_classes: blockifier_state_diff.class_hash_to_compiled_class_hash,
        deprecated_declared_classes: deprecated_declared_class_hash
            .map_or_else(Vec::new, |class_hash| vec![class_hash]),
        nonces: blockifier_state_diff.address_to_nonce,
        replaced_classes,
    })
}

/// Get the storage at the given contract and key in the given state. If there's a given pending
/// storage diffs, apply them on top of the given state.
// TODO(shahak) If the structure of storage diffs changes, remove this function and move its code
// into papyrus_rpc.
pub fn get_storage_at(
    storage_reader: &StorageReader,
    state_number: StateNumber,
    pending_storage_diffs: Option<&IndexMap<ContractAddress, Vec<StorageEntry>>>,
    contract_address: ContractAddress,
    key: StorageKey,
) -> StorageResult<StarkFelt> {
    if let Some(pending_storage_diffs) = pending_storage_diffs {
        if let Some(storage_entries) = pending_storage_diffs.get(&contract_address) {
            // iterating in reverse to get the latest value.
            for StorageEntry { key: other_key, value } in storage_entries.iter().rev() {
                if key == *other_key {
                    return Ok(*value);
                }
            }
        }
    }
    storage_reader.begin_ro_txn()?.get_state_reader()?.get_storage_at(
        state_number,
        &contract_address,
        &key,
    )
}
