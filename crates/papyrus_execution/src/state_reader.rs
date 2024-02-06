#[cfg(test)]
#[path = "state_reader_test.rs"]
mod state_reader_test;

use blockifier::execution::contract_class::{
    ContractClass as BlockifierContractClass,
    ContractClassV0,
    ContractClassV1,
};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use papyrus_common::pending_classes::{ApiContractClass, PendingClassesTrait};
use papyrus_common::state::DeclaredClassHashEntry;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_types_core::felt::Felt;

use crate::execution_utils;
use crate::execution_utils::{get_contract_class, ExecutionUtilsError};
use crate::objects::PendingData;

/// A view into the state at a specific state number.
pub struct ExecutionStateReader {
    pub storage_reader: StorageReader,
    pub state_number: StateNumber,
    pub maybe_pending_data: Option<PendingData>,
    // We want to return a custom error when missing a compiled class, but we need to return
    // Blockifier's error, so we store the missing class's hash in case of error.
    pub missing_compiled_class: Option<ClassHash>,
}

impl BlockifierStateReader for ExecutionStateReader {
    fn get_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        execution_utils::get_storage_at(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            self.state_number,
            self.maybe_pending_data.as_ref().map(|pending_data| &pending_data.storage_diffs),
            contract_address,
            key,
        )
        .map_err(storage_err_to_state_err)
    }

    // Returns the default value if the contract address is not found.
    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(execution_utils::get_nonce_at(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            self.state_number,
            self.maybe_pending_data.as_ref().map(|pending_data| &pending_data.nonces),
            contract_address,
        )
        .map_err(storage_err_to_state_err)?
        .unwrap_or_default())
    }

    // Returns the default value if the contract address is not found.
    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(execution_utils::get_class_hash_at(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            self.state_number,
            self.maybe_pending_data.as_ref().map(|pending_data| {
                (&pending_data.deployed_contracts, &pending_data.replaced_classes)
            }),
            contract_address,
        )
        .map_err(storage_err_to_state_err)?
        .unwrap_or_default())
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: ClassHash,
    ) -> StateResult<BlockifierContractClass> {
        if let Some(pending_casm) = self
            .maybe_pending_data
            .as_ref()
            .and_then(|pending_data| pending_data.classes.get_compiled_class(class_hash))
        {
            return Ok(BlockifierContractClass::V1(
                ContractClassV1::try_from(pending_casm).map_err(StateError::ProgramError)?,
            ));
        }
        if let Some(ApiContractClass::DeprecatedContractClass(pending_deprecated_class)) = self
            .maybe_pending_data
            .as_ref()
            .and_then(|pending_data| pending_data.classes.get_class(class_hash))
        {
            return Ok(BlockifierContractClass::V0(
                ContractClassV0::try_from(pending_deprecated_class)
                    .map_err(StateError::ProgramError)?,
            ));
        }
        match get_contract_class(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            &class_hash,
            self.state_number,
        ) {
            Ok(Some(contract_class)) => Ok(contract_class),
            Ok(None) => Err(StateError::UndeclaredClassHash(class_hash)),
            Err(ExecutionUtilsError::CasmTableNotSynced) => {
                self.missing_compiled_class = Some(class_hash);
                Err(StateError::StateReadError("Casm table not fully synced".to_string()))
            }
            Err(ExecutionUtilsError::ProgramError(err)) => Err(StateError::ProgramError(err)),
            Err(ExecutionUtilsError::StorageError(err)) => Err(storage_err_to_state_err(err)),
        }
    }

    fn get_compiled_class_hash(&mut self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        if let Some(pending_data) = &self.maybe_pending_data {
            for DeclaredClassHashEntry { class_hash: other_class_hash, compiled_class_hash } in
                &pending_data.declared_classes
            {
                if class_hash == *other_class_hash {
                    return Ok(*compiled_class_hash);
                }
            }
        }
        let block_number = self
            .storage_reader
            .begin_ro_txn()
            .map_err(storage_err_to_state_err)?
            .get_state_reader()
            .map_err(storage_err_to_state_err)?
            .get_class_definition_block_number(&class_hash)
            .map_err(storage_err_to_state_err)?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        let state_diff = self
            .storage_reader
            .begin_ro_txn()
            .map_err(storage_err_to_state_err)?
            .get_state_diff(block_number)
            .map_err(storage_err_to_state_err)?
            .ok_or(StateError::StateReadError(format!(
                "Inner storage error. Missing state diff at block {block_number}."
            )))?;

        let compiled_class_hash = state_diff.declared_classes.get(&class_hash).ok_or(
            StateError::StateReadError(format!(
                "Inner storage error. Missing class declaration at block {block_number}, class \
                 {class_hash}."
            )),
        )?;

        Ok(*compiled_class_hash)
    }
}

// Converts a storage error to the error type of the state reader.
fn storage_err_to_state_err(err: StorageError) -> StateError {
    StateError::StateReadError(err.to_string())
}
