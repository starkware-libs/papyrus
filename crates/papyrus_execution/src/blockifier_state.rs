//! Implementation of the [`blockifier`] [`StateReader`] by a [`papyrus_storage`] [`StateReader`].

use blockifier::execution::contract_class::{ContractClass, ContractClassV0};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use papyrus_storage::db::RO;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateNumber, StorageKey};

#[path = "blockifier_state_test.rs"]
#[cfg(test)]
mod blockifier_state_test;

type RawPapyrusStateReader<'env> = papyrus_storage::state::StateReader<'env, RO>;

/// A [`StateReader`] with a fixed block number that implements [`blockifier`] [`StateReader`].

pub struct PapyrusStateReader<'env> {
    pub reader: RawPapyrusStateReader<'env>,
    // Invariant: Read-Only.
    latest_block: BlockNumber,
}

#[allow(dead_code)]
impl<'env> PapyrusStateReader<'env> {
    /// Creates a new [`PapyrusStateReader`] with the given [`StateReader`] and [`BlockNumber`].
    pub fn new(reader: RawPapyrusStateReader<'env>, latest_block: BlockNumber) -> Self {
        Self { reader, latest_block }
    }

    /// Returns the latest block number corresponding to the state reader.
    pub fn latest_block(&self) -> &BlockNumber {
        &self.latest_block
    }
}

impl<'env> StateReader for PapyrusStateReader<'env> {
    fn get_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        let state_number = StateNumber(*self.latest_block());
        self.reader
            .get_storage_at(state_number, &contract_address, &key)
            .map_err(|err| StateError::StateReadError(err.to_string()))
    }

    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let state_number = StateNumber(*self.latest_block());
        match self.reader.get_nonce_at(state_number, &contract_address) {
            Ok(Some(nonce)) => Ok(nonce),
            Ok(None) => Ok(Nonce::default()),
            Err(err) => Err(StateError::StateReadError(err.to_string())),
        }
    }

    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let state_number = StateNumber(*self.latest_block());
        match self.reader.get_class_hash_at(state_number, &contract_address) {
            Ok(Some(class_hash)) => Ok(class_hash),
            Ok(None) => Ok(ClassHash::default()),
            Err(err) => Err(StateError::StateReadError(err.to_string())),
        }
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: &ClassHash,
    ) -> StateResult<ContractClass> {
        let state_number = StateNumber(*self.latest_block());
        match self.reader.get_deprecated_class_definition_at(state_number, class_hash) {
            Ok(Some(starknet_api_contract_class)) => {
                Ok(ContractClassV0::try_from(starknet_api_contract_class)?.into())
            }
            Ok(None) => Err(StateError::UndeclaredClassHash(*class_hash)),
            Err(err) => Err(StateError::StateReadError(err.to_string())),
        }
    }

    fn get_compiled_class_hash(
        &mut self,
        _class_hash: ClassHash,
    ) -> StateResult<CompiledClassHash> {
        todo!()
    }
}
