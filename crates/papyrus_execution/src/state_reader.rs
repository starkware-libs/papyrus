#[cfg(test)]
#[path = "state_reader_test.rs"]
mod state_reader_test;

use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageTxn};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateNumber, StorageKey};

use crate::execution_utils::{get_contract_class, ExecutionUtilsError};

/// A view into the state at a specific state number.
pub struct ExecutionStateReader<'a> {
    pub txn: &'a StorageTxn<'a, RO>,
    pub state_number: StateNumber,
}

impl BlockifierStateReader for ExecutionStateReader<'_> {
    fn get_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        self.txn
            .get_state_reader()
            .map_err(storage_err_to_state_err)?
            .get_storage_at(self.state_number, &contract_address, &key)
            .map_err(storage_err_to_state_err)
    }

    // Returns the default value if the contract address is not found.
    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(self
            .txn
            .get_state_reader()
            .map_err(storage_err_to_state_err)?
            .get_nonce_at(self.state_number, &contract_address)
            .map_err(storage_err_to_state_err)?
            .unwrap_or_default())
    }

    // Returns the default value if the contract address is not found.
    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(self
            .txn
            .get_state_reader()
            .map_err(storage_err_to_state_err)?
            .get_class_hash_at(self.state_number, &contract_address)
            .map_err(storage_err_to_state_err)?
            .unwrap_or_default())
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: &ClassHash,
    ) -> StateResult<BlockifierContractClass> {
        match get_contract_class(self.txn, class_hash, self.state_number) {
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
        let block_number = self
            .txn
            .get_state_reader()
            .map_err(storage_err_to_state_err)?
            .get_class_definition_block_number(&class_hash)
            .map_err(storage_err_to_state_err)?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        let state_diff =
            self.txn.get_state_diff(block_number).map_err(storage_err_to_state_err)?.ok_or(
                StateError::StateReadError(format!(
                    "Inner storage error. Missing state diff at block {block_number}."
                )),
            )?;

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
