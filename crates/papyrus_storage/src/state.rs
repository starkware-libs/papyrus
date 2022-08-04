#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockNumber, ClassHash, ContractAddress, ContractClass, DeclaredContract, DeployedContract,
    IndexedDeclaredContract, IndexedDeployedContract, Nonce, StarkFelt, StateDiff, StateNumber,
    StorageDiff, StorageEntry, StorageKey,
};

use super::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use super::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

pub type DeclaredClassesTable<'env> = TableHandle<'env, ClassHash, IndexedDeclaredContract>;
pub type DeployedContractsTable<'env> = TableHandle<'env, ContractAddress, IndexedDeployedContract>;
pub type ContractStorageTable<'env> =
    TableHandle<'env, (ContractAddress, StorageKey, BlockNumber), StarkFelt>;
pub type NoncesTable<'env> = TableHandle<'env, (ContractAddress, BlockNumber), Nonce>;

// Structure of state data:
// * declared_classes: (class_hash) -> (block_num, contract_class). Each entry specifies at which
//   block was this class declared and with what class definition.
// * deployed_contracts_table: (contract_address) -> (block_num, class_hash). Each entry specifies
//   at which block was this contract deployed and with what class hash. Note that each contract may
//   only be deployed once, so we don't need to support multiple entries per contract address.
// * storage_table: (contract_address, key, block_num) -> (value). Specifies that at `block_num`,
//   the `key` at `contract_address` was changed to `value`. This structure let's us do quick
//   lookup, since the database supports "Get the closet element from  the left". Thus, to lookup
//   the value at a specific block_number, we can search (contract_address, key, block_num), and
//   retrieve the closest from left, which should be the latest update to the value before that
//   block_num.

pub trait StateStorageReader<Mode: TransactionKind> {
    fn get_state_marker(&self) -> StorageResult<BlockNumber>;
    fn get_state_diff(&self, block_number: BlockNumber) -> StorageResult<Option<ThinStateDiff>>;
    fn get_state_reader(&self) -> StorageResult<StateReader<'_, Mode>>;
}

pub trait StateStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_state_diff(
        self,
        block_number: BlockNumber,
        state_diff: StateDiff,
    ) -> StorageResult<Self>;
}

// Invariant: Addresses are strictly increasing.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinStateDiff {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
    pub declared_classes: Vec<ClassHash>,
    pub nonces: Vec<(ContractAddress, Nonce)>,
}

fn split_diff_for_storage(state_diff: StateDiff) -> (ThinStateDiff, Vec<DeclaredContract>) {
    let thin_state_diff = ThinStateDiff {
        deployed_contracts: state_diff.deployed_contracts,
        storage_diffs: state_diff.storage_diffs,
        declared_classes: Vec::from_iter(state_diff.declared_classes.iter().map(|(ch, _)| *ch)),
        nonces: state_diff.nonces,
    };
    let declared_classes = Vec::from_iter(
        state_diff
            .declared_classes
            .into_iter()
            .map(|(ch, co)| DeclaredContract { class_hash: ch, contract_class: co }),
    );
    (thin_state_diff, declared_classes)
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

impl<'env> StateStorageWriter for StorageTxn<'env, RW> {
    fn append_state_diff(
        self,
        block_number: BlockNumber,
        state_diff: StateDiff,
    ) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let nonces_table = self.txn.open_table(&self.tables.nonces)?;
        let deployed_contracts_table = self.txn.open_table(&self.tables.deployed_contracts)?;
        let declared_classes_table = self.txn.open_table(&self.tables.declared_classes)?;
        let storage_table = self.txn.open_table(&self.tables.contract_storage)?;
        let state_diffs_table = self.txn.open_table(&self.tables.state_diffs)?;

        let (thin_state_diff, declared_classes) = split_diff_for_storage(state_diff);

        update_marker(&self.txn, &markers_table, block_number)?;
        // Write state diff.
        state_diffs_table.insert(&self.txn, &block_number, &thin_state_diff)?;
        // Write state.
        write_declared_classes(declared_classes, &self.txn, block_number, &declared_classes_table)?;
        write_deployed_contracts(
            &thin_state_diff,
            &self.txn,
            block_number,
            &deployed_contracts_table,
            &nonces_table,
        )?;
        write_storage_diffs(&thin_state_diff, &self.txn, block_number, &storage_table)?;
        write_nonces(&thin_state_diff, &self.txn, block_number, &nonces_table)?;
        Ok(self)
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
    declared_classes: Vec<DeclaredContract>,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    declared_classes_table: &'env DeclaredClassesTable<'env>,
) -> StorageResult<()> {
    for declared_class in declared_classes {
        // TODO(dan): remove this check after regenesis, in favor of insert().
        if let Some(value) = declared_classes_table.get(txn, &declared_class.class_hash)? {
            if ContractClass::from_byte_vec(&value.contract_class) != declared_class.contract_class
            {
                return Err(StorageError::ClassAlreadyExists {
                    class_hash: declared_class.class_hash,
                });
            }
            continue;
        }
        let value = IndexedDeclaredContract {
            block_number,
            contract_class: declared_class.contract_class.to_byte_vec(),
        };
        let res = declared_classes_table.insert(txn, &declared_class.class_hash, &value);
        match res {
            Ok(()) => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn write_deployed_contracts<'env>(
    state_diff: &ThinStateDiff,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    deployed_contracts_table: &'env DeployedContractsTable<'env>,
    nonces_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for deployed_contract in &state_diff.deployed_contracts {
        let class_hash = deployed_contract.class_hash;
        let value = IndexedDeployedContract { block_number, class_hash };
        deployed_contracts_table.insert(txn, &deployed_contract.address, &value).map_err(
            |err| {
                if matches!(err, DbError::InnerDbError(libmdbx::Error::KeyExist)) {
                    StorageError::ContractAlreadyExists { address: deployed_contract.address }
                } else {
                    StorageError::from(err)
                }
            },
        )?;

        nonces_table
            .insert(txn, &(deployed_contract.address, block_number), &Nonce::default())
            .map_err(|err| {
                if matches!(err, DbError::InnerDbError(libmdbx::Error::KeyExist)) {
                    StorageError::NonceReWrite {
                        contract_address: deployed_contract.address,
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
    state_diff: &ThinStateDiff,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    contracts_table: &'env NoncesTable<'env>,
) -> StorageResult<()> {
    for (contract_address, nonce) in &state_diff.nonces {
        contracts_table.upsert(txn, &(*contract_address, block_number), nonce)?;
    }
    Ok(())
}

fn write_storage_diffs<'env>(
    state_diff: &ThinStateDiff,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    storage_table: &'env ContractStorageTable<'env>,
) -> StorageResult<()> {
    for StorageDiff { address, diff } in &state_diff.storage_diffs {
        for StorageEntry { key, value } in diff {
            storage_table.upsert(txn, &(*address, key.clone(), block_number), value)?;
        }
    }
    Ok(())
}

// A single coherent state at a single point in time,
pub struct StateReader<'env, Mode: TransactionKind> {
    txn: &'env DbTransaction<'env, Mode>,
    declared_classes_table: DeclaredClassesTable<'env>,
    deployed_contracts_table: DeployedContractsTable<'env>,
    nonces_table: NoncesTable<'env>,
    storage_table: ContractStorageTable<'env>,
}
#[allow(dead_code)]
impl<'env, Mode: TransactionKind> StateReader<'env, Mode> {
    pub fn new(txn: &'env StorageTxn<'env, Mode>) -> StorageResult<Self> {
        let declared_classes_table = txn.txn.open_table(&txn.tables.declared_classes)?;
        let deployed_contracts_table = txn.txn.open_table(&txn.tables.deployed_contracts)?;
        let nonces_table = txn.txn.open_table(&txn.tables.nonces)?;
        let storage_table = txn.txn.open_table(&txn.tables.contract_storage)?;
        Ok(StateReader {
            txn: &txn.txn,
            declared_classes_table,
            deployed_contracts_table,
            nonces_table,
            storage_table,
        })
    }
    pub fn get_class_hash_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> StorageResult<Option<ClassHash>> {
        let value = self.deployed_contracts_table.get(self.txn, address)?;
        if let Some(value) = value {
            if state_number.is_after(value.block_number) {
                return Ok(Some(value.class_hash));
            }
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
        let db_key = (*address, key.clone(), first_irrelevant_block);
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
        if let Some(value) = value {
            if state_number.is_after(value.block_number) {
                return Ok(Some(ContractClass::from_byte_vec(&value.contract_class)));
            }
        }
        Ok(None)
    }
}
