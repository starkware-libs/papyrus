//! Implementation of the [`blockifier`] [`StateReader`] by a [`papyrus_storage`].

use std::fmt::Display;

use blockifier::execution::contract_class::{ContractClass, ContractClassV0, ContractClassV1};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::StorageTxn;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateNumber, StorageKey};
use tracing::error;

#[path = "blockifier_state_test.rs"]
#[cfg(test)]
mod blockifier_state_test;

type RawPapyrusStateReader<'env> = papyrus_storage::state::StateReader<'env, RO>;

/// An implementation of [`blockifier`] [`StateReader`] using [`StateReader`] with a fixed block
/// number and a [`StorageTxn`] for accessing compiled classes.
// Invariant: Read-Only.
pub struct PapyrusStateReader<'env> {
    reader: RawPapyrusStateReader<'env>,
    casm_reader: StorageTxn<'env, RO>,
    latest_block: BlockNumber,
}

#[allow(dead_code)]
impl<'env> PapyrusStateReader<'env> {
    /// Creates a new [`PapyrusStateReader`] with the given [`StateReader`], [`StorageTxn`] and
    /// [`BlockNumber`].
    pub fn new(
        reader: RawPapyrusStateReader<'env>,
        casm_reader: StorageTxn<'env, RO>,
        latest_block: BlockNumber,
    ) -> Self {
        Self { reader, casm_reader, latest_block }
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
            .map_err(internal_state_error)
    }

    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let state_number = StateNumber(*self.latest_block());
        match self.reader.get_nonce_at(state_number, &contract_address) {
            Ok(Some(nonce)) => Ok(nonce),
            Ok(None) => Ok(Nonce::default()),
            Err(err) => Err(internal_state_error(err)),
        }
    }

    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let state_number = StateNumber(*self.latest_block());
        match self.reader.get_class_hash_at(state_number, &contract_address) {
            Ok(Some(class_hash)) => Ok(class_hash),
            Ok(None) => Ok(ClassHash::default()),
            Err(err) => Err(internal_state_error(err)),
        }
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: &ClassHash,
    ) -> StateResult<ContractClass> {
        let state_number = StateNumber(*self.latest_block());

        // First, search in the deprecated classes.
        match self.reader.get_deprecated_class_definition_at(state_number, class_hash) {
            Ok(Some(starknet_api_contract_class)) => {
                Ok(ContractClassV0::try_from(starknet_api_contract_class)?.into())
            }
            Err(err) => Err(internal_state_error(err)),
            // In case the class is not found in the deprecated classes, search in the declared
            // classes.
            Ok(None) => match self.casm_reader.get_casm(class_hash) {
                Ok(Some(casm)) => Ok(ContractClassV1::try_from(casm)?.into()),
                Ok(None) => Err(StateError::UndeclaredClassHash(*class_hash)),
                Err(err) => Err(internal_state_error(err)),
            },
        }
    }

    fn get_compiled_class_hash(
        &mut self,
        _class_hash: ClassHash,
    ) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

fn internal_state_error(err: impl Display) -> StateError {
    error!("{}.", err);
    StateError::StateReadError(err.to_string())
}
