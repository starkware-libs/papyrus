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
use blockifier::transaction::objects::TransactionExecutionInfo;
use cairo_vm::types::errors::program_errors::ProgramError;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageTxn};
use starknet_api::core::ClassHash;
use starknet_api::state::StateNumber;
use thiserror::Error;

use crate::objects::TransactionTrace;
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
        ExecutableTransactionInput::Deploy(_) => {
            |execution_info| Ok(TransactionTrace::DeployAccount(execution_info.try_into()?))
        }
        ExecutableTransactionInput::L1Handler(_, _) => {
            |execution_info| Ok(TransactionTrace::L1Handler(execution_info.try_into()?))
        }
    }
}
