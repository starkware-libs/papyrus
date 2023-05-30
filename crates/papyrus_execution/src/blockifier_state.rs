//! Implementation of the [`blockifier`] [`StateReader`] by a [`papyrus_storage`].

use std::fmt::Display;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use native_blockifier::blockifier::execution::contract_class::{
    ContractClass, ContractClassV0, ContractClassV1,
};
use native_blockifier::blockifier::state::errors::StateError;
use native_blockifier::blockifier::state::state_api::{StateReader, StateResult};
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateReader as RawPapyrusStateReader;
use papyrus_storage::{StorageResult, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StateNumber, StorageKey};
use tracing::error;

#[path = "blockifier_state_test.rs"]
#[cfg(test)]
mod blockifier_state_test;

/// An implementation of [`blockifier`] [`StateReader`].
// Invariant: Read-Only.
pub struct PapyrusReader<'env> {
    state: PapyrusStateReader<'env>,
    contract_classes: PapyrusCasmReader<'env>,
}

impl<'env> PapyrusReader<'env> {
    /// Creates a new [`PapyrusReader`] with the given [`StateReader`], [`StorageTxn`] and
    /// [`BlockNumber`].
    pub fn new(
        storage_tx: &'env StorageTxn<'env, RO>,
        state_reader: PapyrusStateReader<'env>,
    ) -> Self {
        let contract_classes = PapyrusCasmReader::new(storage_tx);
        Self { state: state_reader, contract_classes }
    }

    /// Returns a reference to the internal ['RawPapyrusStateReader'].
    pub fn state_reader(&mut self) -> &RawPapyrusStateReader<'env, RO> {
        &self.state.reader
    }
}

/// A wrapper for [`RawPapyrusStateReader`] at a specific [`BlockNumber`].
// Invariant: Read-Only.
pub struct PapyrusStateReader<'env> {
    reader: RawPapyrusStateReader<'env, RO>,
    latest_block: BlockNumber,
}

impl<'env> PapyrusStateReader<'env> {
    /// Creates a new [`PapyrusStateReader`] with the given [`RawPapyrusStateReader`] and
    /// [`BlockNumber`].
    pub fn new(reader: RawPapyrusStateReader<'env, RO>, latest_block: BlockNumber) -> Self {
        Self { reader, latest_block }
    }

    // Returns the latest block number corresponding to the state reader.
    fn latest_block(&self) -> &BlockNumber {
        &self.latest_block
    }
}

/// A wrapper for [`StorageTxn`] that only allows reading [`CasmContractClass`].
pub struct PapyrusCasmReader<'env> {
    txn: &'env StorageTxn<'env, RO>,
}

impl<'env> PapyrusCasmReader<'env> {
    /// Creates a new [`PapyrusCasmReader`] with the given [`StorageTxn`].
    pub fn new(txn: &'env StorageTxn<'env, RO>) -> Self {
        Self { txn }
    }

    // Returns the [`CasmContractClass`] corresponding to the given [`ClassHash`].
    fn get_casm(&self, class_hash: &ClassHash) -> StorageResult<Option<CasmContractClass>> {
        self.txn.get_casm(class_hash)
    }
}

impl<'env> StateReader for PapyrusReader<'env> {
    fn get_storage_at(
        &mut self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        let state_number = StateNumber(*self.state.latest_block());
        self.state_reader()
            .get_storage_at(state_number, &contract_address, &key)
            .map_err(internal_state_error)
    }

    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let state_number = StateNumber(*self.state.latest_block());
        match self.state_reader().get_nonce_at(state_number, &contract_address) {
            Ok(Some(nonce)) => Ok(nonce),
            Ok(None) => Ok(Nonce::default()),
            Err(err) => Err(internal_state_error(err)),
        }
    }

    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let state_number = StateNumber(*self.state.latest_block());
        match self.state_reader().get_class_hash_at(state_number, &contract_address) {
            Ok(Some(class_hash)) => Ok(class_hash),
            Ok(None) => Ok(ClassHash::default()),
            Err(err) => Err(internal_state_error(err)),
        }
    }

    fn get_compiled_contract_class(
        &mut self,
        class_hash: &ClassHash,
    ) -> StateResult<ContractClass> {
        let state_number = StateNumber(*self.state.latest_block());

        // First, search in the deprecated classes.
        match self.state_reader().get_deprecated_class_definition_at(state_number, class_hash) {
            Ok(Some(starknet_api_contract_class)) => {
                Ok(ContractClassV0::try_from(starknet_api_contract_class)?.into())
            }
            Err(err) => Err(internal_state_error(err)),
            // In case the class is not found in the deprecated classes, search in the declared
            // classes.
            Ok(None) => {
                let Some(class_declaration_block_number) = self
                    .state_reader()
                    .get_class_definition_block_number(class_hash)
                    .map_err(internal_state_error)? else {
                        // The class hash is not declared.
                        return Err(StateError::UndeclaredClassHash(*class_hash));
                    };
                if class_declaration_block_number > *self.state.latest_block() {
                    // The class is declared in a future block.
                    return Err(StateError::UndeclaredClassHash(*class_hash));
                }
                let Some(casm) = self.
                    contract_classes.
                    get_casm(class_hash).
                    map_err(internal_state_error)? else {
                        // The class is declared but not found.
                        return Err(internal_state_error("block number found in \
                        declared classes block table but corresponding contract class is not found \
                        in declared classes table."
                      ));
                    };
                Ok(ContractClassV1::try_from(casm)?.into())
            }
        }
    }

    fn get_compiled_class_hash(
        &mut self,
        _class_hash: ClassHash,
    ) -> StateResult<CompiledClassHash> {
        todo!()
    }
}

// Logs and returns a [`StateError`] with the given error.
fn internal_state_error(err: impl Display) -> StateError {
    error!("{}.", err);
    StateError::StateReadError(err.to_string())
}
