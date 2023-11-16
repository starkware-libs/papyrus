#[cfg(test)]
#[path = "state_reader_test.rs"]
mod state_reader_test;

use blockifier::execution::contract_class::{
    ContractClass as BlockifierContractClass,
    ContractClassV1,
};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use papyrus_common::pending_classes::{PendingClasses, PendingClassesTrait};
use papyrus_common::state::DeclaredClassHashEntry;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateNumber, StorageKey};

use crate::execution_utils;
use crate::execution_utils::{get_contract_class, ExecutionUtilsError};
use crate::objects::PendingStateDiff;

/// A view into the state at a specific state number.
pub struct ExecutionStateReader {
    pub storage_reader: StorageReader,
    pub state_number: StateNumber,
    pub maybe_pending_state_diff: Option<PendingStateDiff>,
    pub maybe_pending_classes: Option<PendingClasses>,
}

impl BlockifierStateReader for ExecutionStateReader {
    fn get_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        execution_utils::get_storage_at(
            &self.storage_reader,
            self.state_number,
            self.maybe_pending_state_diff
                .as_ref()
                .map(|pending_state_diff| &pending_state_diff.storage_diffs),
            contract_address,
            key,
        )
        .map_err(storage_err_to_state_err)
    }

    // Returns the default value if the contract address is not found.
    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        execution_utils::get_nonce_at(
            &self.storage_reader,
            self.state_number,
            self.maybe_pending_state_diff
                .as_ref()
                .map(|pending_state_diff| &pending_state_diff.nonces),
            contract_address,
        )
        .map_err(storage_err_to_state_err)
        .map(|maybe_nonce| maybe_nonce.unwrap_or_default())
    }

    // Returns the default value if the contract address is not found.
    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        execution_utils::get_class_hash_at(
            &self.storage_reader,
            self.state_number,
            self.maybe_pending_state_diff
                .as_ref()
                .map(|pending_state_diff| &pending_state_diff.deployed_contracts),
            contract_address,
        )
        .map_err(storage_err_to_state_err)
        .map(|maybe_class_hash| maybe_class_hash.unwrap_or_default())
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: &ClassHash,
    ) -> StateResult<BlockifierContractClass> {
        if let Some(pending_casm) = self
            .maybe_pending_classes
            .as_ref()
            .and_then(|pending_classes| pending_classes.get_compiled_class(*class_hash))
            .clone()
        {
            return Ok(BlockifierContractClass::V1(
                ContractClassV1::try_from(pending_casm).map_err(StateError::ProgramError)?,
            ));
        }
        match get_contract_class(
            &self.storage_reader.begin_ro_txn().map_err(storage_err_to_state_err)?,
            class_hash,
            self.state_number,
        ) {
            Ok(Some(contract_class)) => Ok(contract_class),
            Ok(None) => Err(StateError::UndeclaredClassHash(*class_hash)),
            Err(ExecutionUtilsError::CasmTableNotSynced) => {
                Err(StateError::StateReadError("Casm table not fully synced".to_string()))
            }
            Err(ExecutionUtilsError::ProgramError(err)) => Err(StateError::ProgramError(err)),
            Err(ExecutionUtilsError::StorageError(err)) => Err(storage_err_to_state_err(err)),
        }
    }

    fn get_compiled_class_hash(&mut self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        if let Some(pending_state_diff) = &self.maybe_pending_state_diff {
            for DeclaredClassHashEntry { class_hash: other_class_hash, compiled_class_hash } in
                &pending_state_diff.declared_classes
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
