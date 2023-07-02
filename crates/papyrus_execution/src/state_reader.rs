use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageTxn;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress};
use starknet_api::state::StateNumber;

use crate::execution_utils::{get_contract_class, ExecutionUtilsError};

pub struct ExecutionStateReader<'a> {
    pub txn: &'a StorageTxn<'a, RO>,
    pub state_number: StateNumber,
}

impl BlockifierStateReader for ExecutionStateReader<'_> {
    fn get_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: starknet_api::state::StorageKey,
    ) -> blockifier::state::state_api::StateResult<starknet_api::hash::StarkFelt> {
        self.txn
            .get_state_reader()
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .get_storage_at(self.state_number, &contract_address, &key)
            .map_err(|err| StateError::StateReadError(err.to_string()))
    }

    fn get_nonce_at(
        &mut self,
        contract_address: ContractAddress,
    ) -> blockifier::state::state_api::StateResult<starknet_api::core::Nonce> {
        self.txn
            .get_state_reader()
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .get_nonce_at(self.state_number, &contract_address)
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::StateReadError(
                "Nonce not found, contract_address = {contract_address}.".to_string(),
            ))
    }

    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.txn
            .get_state_reader()
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .get_class_hash_at(self.state_number, &contract_address)
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::StateReadError(
                "Class hash not found, contract_address = {contract_address}, state_number = \
                 {self.state_number:?}."
                    .to_string(),
            ))
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: &ClassHash,
    ) -> StateResult<BlockifierContractClass> {
        match get_contract_class(self.txn, class_hash, self.state_number) {
            Ok(Some(contract_class)) => Ok(contract_class),
            Ok(None) => Err(StateError::UndeclaredClassHash(*class_hash)),
            Err(ExecutionUtilsError::ProgramError(err)) => Err(StateError::ProgramError(err)),
            Err(ExecutionUtilsError::StorageError(err)) => {
                Err(StateError::StateReadError(err.to_string()))
            }
        }
    }

    fn get_compiled_class_hash(&mut self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        let block_number = self
            .txn
            .get_state_reader()
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .get_class_definition_block_number(&class_hash)
            .map_err(|err| StateError::StateReadError(err.to_string()))?
            .ok_or(StateError::UndeclaredClassHash(class_hash))?;

        let state_diff = self
            .txn
            .get_state_diff(block_number)
            .map_err(|err| StateError::StateReadError(err.to_string()))?
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
