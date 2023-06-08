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

#[derive(Debug, Error)]
pub enum ExecutionUtilsError {
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
}

pub fn get_contract_class(
    txn: &StorageTxn<RO>,
    class_hash: ClassHash,
    state_number: StateNumber,
) -> Result<Option<BlockifierContractClass>, ExecutionUtilsError> {
    if let Some(casm) = txn.get_casm(class_hash)? {
        return Ok(Some(BlockifierContractClass::V1(
            ContractClassV1::try_from(casm).map_err(ExecutionUtilsError::ProgramError)?,
        )));
    }
    if let Some(contract_class) = txn
        .get_state_reader()
        .map_err(ExecutionUtilsError::StorageError)?
        .get_deprecated_class_definition_at(state_number, &class_hash)?
    {
        return Ok(Some(BlockifierContractClass::V0(
            ContractClassV0::try_from(contract_class).map_err(ExecutionUtilsError::ProgramError)?,
        )));
    }
    Ok(None)
}
