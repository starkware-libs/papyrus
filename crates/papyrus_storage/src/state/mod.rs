pub mod data;
#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::collections::HashSet;

use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkFelt;
use starknet_api::state::{ContractClass, StateDiff, StateNumber, StorageKey, ThinStateDiff};
use tracing::debug;

use crate::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use crate::state::data::{
    IndexedContractClass, IndexedDeployedContract, IndexedDeprecatedContractClass,
};
use crate::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

type DeclaredClassesTable<'env> = TableHandle<'env, ClassHash, IndexedContractClass>;
type DeprecatedDeclaredClassesTable<'env> =
    TableHandle<'env, ClassHash, IndexedDeprecatedContractClass>;
type DeployedContractsTable<'env> = TableHandle<'env, ContractAddress, IndexedDeployedContract>;
type ContractStorageTable<'env> =
    TableHandle<'env, (ContractAddress, StorageKey, BlockNumber), StarkFelt>;
type NoncesTable<'env> = TableHandle<'env, (ContractAddress, BlockNumber), Nonce>;
type ReplacedClassesTable<'env> = TableHandle<'env, (ContractAddress, BlockNumber), ClassHash>;

// Structure of state data:
// * declared_classes_table: (class_hash) -> (block_num, contract_class). Each entry specifies at
//   which block was this class declared and with what class definition. For Cairo 1 class
//   definitions.
// * deprecated_declared_classes_table: (class_hash) -> (block_num, deprecated_contract_class). Each
//   entry specifies at which block was this class declared and with what class definition. For
//   Cairo 0 class definitions.
// * deployed_contracts_table: (contract_address) -> (block_num, class_hash). Each entry specifies
//   at which block was this contract deployed and with what class hash. Note that each contract may
//   only be deployed once, so we don't need to support multiple entries per contract address.
// * storage_table: (contract_address, key, block_num) -> (value). Specifies that at `block_num`,
//   the `key` at `contract_address` was changed to `value`. This structure let's us do quick
//   lookup, since the database supports "Get the closet element from  the left". Thus, to lookup
//   the value at a specific block_number, we can search (contract_address, key, block_num), and
//   retrieve the closest from left, which should be the latest update to the value before that
//   block_num.
// * nonces_table: (contract_address, block_num) -> (nonce). Specifies that at `block_num`, the
//   nonce of `contract_address` was changed to `nonce`.
// * replaced_classes_table: (contract_address, block_num) -> (class_hash). Specifies that at
//   `block_num`, the class of `contract_address` was changed to the class with `class_hash`.

pub trait StateStorageReader<Mode: TransactionKind> {
    fn get_state_marker(&self) -> StorageResult<BlockNumber>;
    fn get_state_diff(&self, block_number: BlockNumber) -> StorageResult<Option<ThinStateDiff>>;
    fn get_state_reader(&self) -> StorageResult<StateReader<'_, Mode>>;
}

type RevertedStateDiff = (
    ThinStateDiff,
    IndexMap<ClassHash, ContractClass>,
    IndexMap<ClassHash, DeprecatedContractClass>,
);

pub trait StateStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_state_diff(
        self,
        block_number: BlockNumber,
        state_diff: StateDiff,
        // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
        // Class definitions of deployed contracts with classes that were not declared in this
        // state diff.
        // Note: Since 0.11 only deprecated classes can be implicitly declared by contract
        // deployment, so there is no need to pass the classes of deployed contracts if they are of
        // the new version.
        deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
    ) -> StorageResult<Self>;

    fn revert_state_diff(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedStateDiff>)>;
}

impl<'env, Mode: TransactionKind> StateStorageReader<Mode> for StorageTxn<'env, Mode> {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_state_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::State)?.unwrap_or_default())
    }
    fn get_state_diff(&self, block_number: BlockNumber) -> StorageResult<Option<ThinStateDiff>> {
        let state_diffs_table = self.txn.open_table(&self.tables.state_diffs)?;
        let state_diff = state_diffs_table.get(&self.txn, &block_number)?;
        Ok(state_diff)
    }
    fn get_state_reader(&self) -> StorageResult<StateReader<'_, Mode>> {
        StateReader::new(self)
    }
}

/// A single coherent state at a single point in time,
pub struct StateReader<'env, Mode: TransactionKind> {
    txn: &'env DbTransaction<'env, Mode>,
    declared_classes_table: DeclaredClassesTable<'env>,
    deprecated_declared_classes_table: DeprecatedDeclaredClassesTable<'env>,
    deployed_contracts_table: DeployedContractsTable<'env>,
    nonces_table: NoncesTable<'env>,
    replaced_classes_table: ReplacedClassesTable<'env>,
    storage_table: ContractStorageTable<'env>,
}

#[allow(dead_code)]
impl<'env, Mode: TransactionKind> StateReader<'env, Mode> {
    fn new(txn: &'env StorageTxn<'env, Mode>) -> StorageResult<Self> {
        let declared_classes_table = txn.txn.open_table(&txn.tables.declared_classes)?;
        let deprecated_declared_classes_table =
            txn.txn.open_table(&txn.tables.deprecated_declared_classes)?;
        let deployed_contracts_table = txn.txn.open_table(&txn.tables.deployed_contracts)?;
        let nonces_table = txn.txn.open_table(&txn.tables.nonces)?;
        let storage_table = txn.txn.open_table(&txn.tables.contract_storage)?;
        let replaced_classes_table = txn.txn.open_table(&txn.tables.replaced_classes)?;
        Ok(StateReader {
            txn: &txn.txn,
            declared_classes_table,
            deprecated_declared_classes_table,
            deployed_contracts_table,
            nonces_table,
            replaced_classes_table,
            storage_table,
        })
    }

    // Returns the latest class hash of the contract, before state_number.
    // If the class wasn't replaced before state_number, returns None.
    // Note: None means that the class in deployed_contracts table was not replaced before
    // state_number, it could still be declared before state_number - need to check against the
    // value in deployed_contracts.
    fn get_replaced_class_hash(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<ClassHash>> {
        let first_irrelevant_block: BlockNumber = state_number.block_after();
        let db_key = (*address, first_irrelevant_block);
        let mut cursor = self.replaced_classes_table.cursor(self.txn)?;
        cursor.lower_bound(&db_key)?;
        let res = cursor.prev()?;

        match res {
            // Class was never replaced before state_number.
            None => Ok(None),
            Some(((got_address, _), _)) if got_address != *address => Ok(None),
            // Class was replaced to new_class_hash.
            Some((_, new_class_hash)) => Ok(Some(new_class_hash)),
        }
    }

    pub fn get_class_hash_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<ClassHash>> {
        // Check if the class was replaced before state_number.
        if let Some(class_hash) = self.get_replaced_class_hash(state_number, address)? {
            return Ok(Some(class_hash));
        }

        // The class wasn't replaced, check in deployed_contracts.
        let value = self.deployed_contracts_table.get(self.txn, address)?;
        let Some(value) = value else {return Ok(None)};
        if state_number.is_after(value.block_number) {
            return Ok(Some(value.class_hash));
        }
        Ok(None)
    }

    pub fn get_nonce_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<Nonce>> {
        // State diff updates are indexed by the block_number at which they occurred.
        let first_irrelevant_block: BlockNumber = state_number.block_after();
        // The relevant update is the last update strictly before `first_irrelevant_block`.
        let db_key = (*address, first_irrelevant_block);
        // Find the previous db item.
        let mut cursor = self.nonces_table.cursor(self.txn)?;
        cursor.lower_bound(&db_key)?;
        let res = cursor.prev()?;
        match res {
            None => Ok(None),
            Some(((got_address, _got_block_number), value)) => {
                if got_address != *address {
                    // The previous item belongs to different address, which means there is no
                    // previous state diff for this item.
                    return Ok(None);
                };
                // The previous db item indeed belongs to this address and key.
                Ok(Some(value))
            }
        }
    }

    pub fn get_storage_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
        key: &StorageKey,
    ) -> StorageResult<StarkFelt> {
        // The updates to the storage key are indexed by the block_number at which they occurred.
        let first_irrelevant_block: BlockNumber = state_number.block_after();
        // The relevant update is the last update strictly before `first_irrelevant_block`.
        let db_key = (*address, *key, first_irrelevant_block);
        // Find the previous db item.
        let mut cursor = self.storage_table.cursor(self.txn)?;
        cursor.lower_bound(&db_key)?;
        let res = cursor.prev()?;
        match res {
            None => Ok(StarkFelt::default()),
            Some(((got_address, got_key, _got_block_number), value)) => {
                if got_address != *address || got_key != *key {
                    // The previous item belongs to different key, which means there is no
                    // previous state diff for this item.
                    return Ok(StarkFelt::default());
                };
                // The previous db item indeed belongs to this address and key.
                Ok(value)
            }
        }
    }

    pub fn get_class_definition_at(
        &self,
        state_number: StateNumber,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<ContractClass>> {
        let value = self.declared_classes_table.get(self.txn, class_hash)?;
        let Some(value) = value else {return Ok(None)};
        if state_number.is_after(value.block_number) {
            return Ok(Some(value.contract_class));
        }
        Ok(None)
    }

    pub fn get_deprecated_class_definition_at(
        &self,
        state_number: StateNumber,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<DeprecatedContractClass>> {
        let value = self.deprecated_declared_classes_table.get(self.txn, class_hash)?;
        if let Some(value) = value {
            if state_number.is_after(value.block_number) {
                return Ok(Some(value.contract_class));
            }
        }
        Ok(None)
    }
}

impl<'env> StateStorageWriter for StorageTxn<'env, RW> {
    fn append_state_diff(
        self,
        block_number: BlockNumber,
        state_diff: StateDiff,
        mut deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
    ) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let nonces_table = self.txn.open_table(&self.tables.nonces)?;
        let deployed_contracts_table = self.txn.open_table(&self.tables.deployed_contracts)?;
        let declared_classes_table = self.txn.open_table(&self.tables.declared_classes)?;
        let deprecated_declared_classes_table =
            self.txn.open_table(&self.tables.deprecated_declared_classes)?;
        let storage_table = self.txn.open_table(&self.tables.contract_storage)?;
        let replaced_classes_table = self.txn.open_table(&self.tables.replaced_classes)?;
        let state_diffs_table = self.txn.open_table(&self.tables.state_diffs)?;

        update_marker(&self.txn, &markers_table, block_number)?;

        // Write state except declared classes.
        write_deployed_contracts(
            &state_diff.deployed_contracts,
            &self.txn,
            block_number,
            &deployed_contracts_table,
            &nonces_table,
        )?;
        write_storage_diffs(&state_diff.storage_diffs, &self.txn, block_number, &storage_table)?;
        write_nonces(&state_diff.nonces, &self.txn, block_number, &nonces_table)?;
        write_replaced_classes(
            &state_diff.replaced_classes,
            &self.txn,
            block_number,
            &replaced_classes_table,
        )?;

        // Write state diff.
        let (thin_state_diff, declared_classes, deprecated_declared_classes) =
            ThinStateDiff::from_state_diff(state_diff);
        state_diffs_table.insert(&self.txn, &block_number, &thin_state_diff)?;

        // Write declared classes.
        write_declared_classes(declared_classes, &self.txn, block_number, &declared_classes_table)?;

        // Write deprecated declared classes.
        if !deployed_contract_class_definitions.is_empty() {
            // TODO(anatg): Remove this after regenesis.
            if !deprecated_declared_classes.is_empty() {
                deployed_contract_class_definitions.extend(deprecated_declared_classes);
                //  TODO(anatg): Add a test for this (should fail if not sorted here).
                deployed_contract_class_definitions.sort_unstable_keys();
            }
            write_deprecated_declared_classes(
                deployed_contract_class_definitions,
                &self.txn,
                block_number,
                &deprecated_declared_classes_table,
            )?;
        } else {
            write_deprecated_declared_classes(
                deprecated_declared_classes,
                &self.txn,
                block_number,
                &deprecated_declared_classes_table,
            )?;
        }

        Ok(self)
    }

    fn revert_state_diff(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedStateDiff>)> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let declared_classes_table = self.txn.open_table(&self.tables.declared_classes)?;
        let deprecated_declared_classes_table =
            self.txn.open_table(&self.tables.deprecated_declared_classes)?;
        let deployed_contracts_table = self.txn.open_table(&self.tables.deployed_contracts)?;
        let nonces_table = self.txn.open_table(&self.tables.nonces)?;
        let storage_table = self.txn.open_table(&self.tables.contract_storage)?;
        let state_diffs_table = self.txn.open_table(&self.tables.state_diffs)?;
        let replaced_classes_table = self.txn.open_table(&self.tables.replaced_classes)?;

        let current_state_marker = self.get_state_marker()?;

        // Reverts only the last state diff.
        if current_state_marker != block_number.next() {
            debug!(
                "Attempt to revert a non-existing / old state diff of block {}. Returning without \
                 an action.",
                block_number
            );
            return Ok((self, None));
        }

        let thin_state_diff = self
            .get_state_diff(block_number)?
            .expect("Missing state diff for block {block_number}.");
        markers_table.upsert(&self.txn, &MarkerKind::State, &block_number)?;
        let deleted_classes =
            delete_declared_classes(&self.txn, &thin_state_diff, &declared_classes_table)?;
        let deleted_deprecated_classes = delete_deprecated_declared_classes(
            &self.txn,
            block_number,
            &thin_state_diff,
            &deprecated_declared_classes_table,
        )?;
        delete_deployed_contracts(
            &self.txn,
            block_number,
            &thin_state_diff,
            &deployed_contracts_table,
            &nonces_table,
        )?;
        delete_storage_diffs(&self.txn, block_number, &thin_state_diff, &storage_table)?;
        delete_nonces(&self.txn, block_number, &thin_state_diff, &nonces_table)?;
        state_diffs_table.delete(&self.txn, &block_number)?;
        delete_replaced_classes(
            &self.txn,
            block_number,
            &thin_state_diff,
            &replaced_classes_table,
        )?;

        Ok((self, Some((thin_state_diff, deleted_classes, deleted_deprecated_classes))))
    }
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    // Make sure marker is consistent.
    let state_marker = markers_table.get(txn, &MarkerKind::State)?.unwrap_or_default();
    if state_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: state_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::State, &block_number.next())?;
    Ok(())
}

fn write_declared_classes<'env>(
    declared_classes: IndexMap<ClassHash, ContractClass>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    declared_classes_table: &'env DeclaredClassesTable<'env>,
) -> StorageResult<()> {
    for (class_hash, contract_class) in declared_classes {
        let value = IndexedContractClass { block_number, contract_class };
        let res = declared_classes_table.insert(txn, &class_hash, &value);
        match res {
            Ok(()) => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn write_deprecated_declared_classes<'env>(
    deprecated_declared_classes: IndexMap<ClassHash, DeprecatedContractClass>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    deprecated_declared_classes_table: &'env DeprecatedDeclaredClassesTable<'env>,
) -> StorageResult<()> {
    for (class_hash, deprecated_contract_class) in deprecated_declared_classes {
        // TODO(dan): remove this check after regenesis, in favor of insert().
        if let Some(value) = deprecated_declared_classes_table.get(txn, &class_hash)? {
            if value.contract_class != deprecated_contract_class {
                return Err(StorageError::ClassAlreadyExists { class_hash });
            }
            continue;
        }
        let value = IndexedDeprecatedContractClass {
            block_number,
            contract_class: deprecated_contract_class,
        };
        let res = deprecated_declared_classes_table.insert(txn, &class_hash, &value);
        match res {
            Ok(()) => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn write_deployed_contracts<'env>(
    deployed_contracts: &IndexMap<ContractAddress, ClassHash>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    deployed_contracts_table: &'env DeployedContractsTable<'env>,
    nonces_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for (address, class_hash) in deployed_contracts {
        let value = IndexedDeployedContract { block_number, class_hash: *class_hash };
        deployed_contracts_table.insert(txn, address, &value).map_err(|err| {
            if matches!(err, DbError::Inner(libmdbx::Error::KeyExist)) {
                StorageError::ContractAlreadyExists { address: *address }
            } else {
                StorageError::from(err)
            }
        })?;

        nonces_table.insert(txn, &(*address, block_number), &Nonce::default()).map_err(|err| {
            if matches!(err, DbError::Inner(libmdbx::Error::KeyExist)) {
                StorageError::NonceReWrite {
                    contract_address: *address,
                    nonce: Nonce::default(),
                    block_number,
                }
            } else {
                StorageError::from(err)
            }
        })?;
    }
    Ok(())
}

fn write_nonces<'env>(
    nonces: &IndexMap<ContractAddress, Nonce>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    contracts_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for (contract_address, nonce) in nonces {
        contracts_table.upsert(txn, &(*contract_address, block_number), nonce)?;
    }
    Ok(())
}

fn write_replaced_classes<'env>(
    replaced_classes: &IndexMap<ContractAddress, ClassHash>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    replaced_classes_table: &'env ReplacedClassesTable<'env>,
) -> StorageResult<()> {
    for (contract_address, class_hash) in replaced_classes {
        replaced_classes_table.insert(txn, &(*contract_address, block_number), class_hash)?;
    }
    Ok(())
}

fn write_storage_diffs<'env>(
    storage_diffs: &IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    storage_table: &'env ContractStorageTable<'env>,
) -> StorageResult<()> {
    for (address, storage_entries) in storage_diffs {
        for (key, value) in storage_entries {
            storage_table.upsert(txn, &(*address, *key, block_number), value)?;
        }
    }
    Ok(())
}

fn delete_declared_classes<'env>(
    txn: &'env DbTransaction<'env, RW>,
    thin_state_diff: &ThinStateDiff,
    declared_classes_table: &'env DeclaredClassesTable<'env>,
) -> StorageResult<IndexMap<ClassHash, ContractClass>> {
    let mut deleted_data = IndexMap::new();
    for class_hash in thin_state_diff.declared_classes.keys() {
        let IndexedContractClass { block_number: _, contract_class } = declared_classes_table
            .get(txn, class_hash)?
            .expect("Missing declared class {class_hash:#?}.");
        deleted_data.insert(*class_hash, contract_class);
        declared_classes_table.delete(txn, class_hash)?;
    }

    Ok(deleted_data)
}

fn delete_deprecated_declared_classes<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    deprecated_declared_classes_table: &'env DeprecatedDeclaredClassesTable<'env>,
) -> StorageResult<IndexMap<ClassHash, DeprecatedContractClass>> {
    // Class hashes of the contracts that were deployed in this block.
    let deployed_contracts_class_hashes = thin_state_diff.deployed_contracts.values();

    // Merge the class hashes from the state diff and from the deployed contracts into a single
    // unique set.
    let class_hashes: HashSet<&ClassHash> = thin_state_diff
        .deprecated_declared_classes
        .iter()
        .chain(deployed_contracts_class_hashes)
        .collect();

    let mut deleted_data = IndexMap::new();
    for class_hash in class_hashes {
        // If the class is not in the deprecated classes table, it means that the hash is of a
        // deployed contract of a new class type. We don't need to delete these classes because
        // since 0.11 new classes must be explicitly declared. Therefore we can skip hashes that we
        // don't find in the deprecated classes table.
        if let Some(IndexedDeprecatedContractClass {
            block_number: declared_block_number,
            contract_class,
        }) = deprecated_declared_classes_table.get(txn, class_hash)?
        {
            // If the class was declared in a different block then we should'nt delete it.
            if block_number == declared_block_number {
                deleted_data.insert(*class_hash, contract_class);
                deprecated_declared_classes_table.delete(txn, class_hash)?;
            }
        }
    }

    Ok(deleted_data)
}

fn delete_deployed_contracts<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    deployed_contracts_table: &'env DeployedContractsTable<'env>,
    nonces_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for contract_address in thin_state_diff.deployed_contracts.keys() {
        deployed_contracts_table.delete(txn, contract_address)?;
        nonces_table.delete(txn, &(*contract_address, block_number))?;
    }
    Ok(())
}

fn delete_storage_diffs<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    storage_table: &'env ContractStorageTable<'env>,
) -> StorageResult<()> {
    for (address, storage_entries) in &thin_state_diff.storage_diffs {
        for (key, _) in storage_entries {
            storage_table.delete(txn, &(*address, *key, block_number))?;
        }
    }
    Ok(())
}

fn delete_nonces<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    contracts_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for contract_address in thin_state_diff.nonces.keys() {
        contracts_table.delete(txn, &(*contract_address, block_number))?;
    }
    Ok(())
}

fn delete_replaced_classes<'env>(
    txn: &'env DbTransaction<'env, RW>,
    block_number: BlockNumber,
    thin_state_diff: &ThinStateDiff,
    replaced_classes_table: &'env ReplacedClassesTable<'env>,
) -> StorageResult<()> {
    for contract_address in thin_state_diff.replaced_classes.keys() {
        replaced_classes_table.delete(txn, &(*contract_address, block_number))?;
    }
    Ok(())
}
