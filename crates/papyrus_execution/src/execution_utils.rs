use blockifier::execution::contract_class::{
    ContractClass as BlockifierContractClass, ContractClassV0, ContractClassV1,
};
use cairo_vm::types::errors::program_errors::ProgramError;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageTxn};
use starknet_api::core::ClassHash;
use starknet_api::state::StateNumber;
use thiserror::Error;

// An error that can occur during the use of the execution utils.
#[derive(Debug, Error)]
pub enum ExecutionUtilsError {
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Casm table not fully synced")]
    CasmTableNotSynced,
}

pub fn get_contract_class(
    txn: &StorageTxn<'_, RO>,
    class_hash: &ClassHash,
    state_number: StateNumber,
) -> Result<Option<BlockifierContractClass>, ExecutionUtilsError> {
    match txn.get_state_reader()?.get_class_definition_block_number(class_hash)? {
        Some(block_number) if state_number.is_before(block_number) => return Ok(None),
        Some(_block_number) => {
            let Some(casm) = txn.get_casm(class_hash)? else { return Err(ExecutionUtilsError::CasmTableNotSynced) };
            return Ok(Some(BlockifierContractClass::V1(
                ContractClassV1::try_from(casm).map_err(ExecutionUtilsError::ProgramError)?,
            )));
        }
        None => {}
    };

    let Some(deprecated_class) =
        txn.get_state_reader()?.get_deprecated_class_definition_at(state_number, class_hash)? else {return Ok(None);};
    Ok(Some(BlockifierContractClass::V0(
        ContractClassV0::try_from(deprecated_class).map_err(ExecutionUtilsError::ProgramError)?,
    )))
}
