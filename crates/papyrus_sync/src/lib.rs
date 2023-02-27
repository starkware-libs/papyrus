#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;

mod sources;

use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use futures_util::{pin_mut, select, Stream, StreamExt};
use indexmap::IndexMap;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::{OmmerStorageReader, OmmerStorageWriter};
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tracing::{debug, error, info, trace, warn};

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct SyncConfig {
    pub block_propagation_sleep_duration: Duration,
    pub recoverable_error_sleep_duration: Duration,
}

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct GenericStateSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    writer: StorageWriter,
}

pub type StateSyncResult = Result<(), StateSyncError>;

#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
    #[error(
        "Parent block hash of block {block_number} is not consistent with the stored block. \
         Expected {expected_parent_block_hash}, found {stored_parent_block_hash}."
    )]
    ParentBlockHashMismatch {
        block_number: BlockNumber,
        expected_parent_block_hash: BlockHash,
        stored_parent_block_hash: BlockHash,
    },
    #[error(
        "Received state diff of block {block_number} and block hash {block_hash}, didn't find a \
         matching header (neither in the ommer headers)."
    )]
    StateDiffWithoutMatchingHeader { block_number: BlockNumber, block_hash: BlockHash },
}

#[allow(clippy::large_enum_variant)]
pub enum SyncEvent {
    BlockAvailable {
        block_number: BlockNumber,
        block: Block,
    },
    StateDiffAvailable {
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
        // Class definitions of deployed contracts with classes that were not declared in this
        // state diff.
        deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
    },
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) -> StateSyncResult {
        info!("State sync started.");
        loop {
            match self.sync_while_ok().await {
                Err(StateSyncError::ParentBlockHashMismatch {
                    block_number,
                    expected_parent_block_hash,
                    stored_parent_block_hash,
                }) => {
                    // A revert detected, log and restart sync loop.
                    info!(
                        "Detected revert while processing block {}. Parent hash of the incoming \
                         block is {}, current block hash is {}.",
                        block_number, expected_parent_block_hash, stored_parent_block_hash
                    );
                    continue;
                }
                // A recoverable error occurred. Sleep and try syncing again.
                Err(err) if is_recoverable(&err) => {
                    warn!("{}", err);
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    continue;
                }
                // Unrecoverable errors.
                Err(err) => {
                    error!("{}", err);
                    return Err(err);
                }
                Ok(_) => {
                    unreachable!("Sync should either return with an error or continue forever.")
                }
            }
        }

        // Whitelisting of errors from which we might be able to recover.
        fn is_recoverable(err: &StateSyncError) -> bool {
            match err {
                StateSyncError::CentralSourceError(_) => true,
                StateSyncError::StorageError(storage_err)
                    if matches!(storage_err, StorageError::InnerError(_)) =>
                {
                    true
                }
                StateSyncError::StateDiffWithoutMatchingHeader {
                    block_number: _,
                    block_hash: _,
                } => true,
                _ => false,
            }
        }
    }

    // Sync until encountering an error:
    //  1. If needed, revert blocks from the end of the chain.
    //  2. Create infinite block and state diff streams to fetch data from the central source.
    //  3. Fetch data from the streams with unblocking wait while there is no new data.
    async fn sync_while_ok(&mut self) -> StateSyncResult {
        self.handle_block_reverts().await?;
        let block_stream = stream_new_blocks(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.block_propagation_sleep_duration,
        )
        .fuse();
        let state_diff_stream = stream_new_state_diffs(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.block_propagation_sleep_duration,
        )
        .fuse();
        pin_mut!(block_stream, state_diff_stream);

        loop {
            debug!("Selecting between block sync and state diff sync.");
            let sync_event = select! {
              res = block_stream.next() => res,
              res = state_diff_stream.next() => res,
              complete => break,
            }
            .expect("Received None as a sync event.")?;
            self.process_sync_event(sync_event).await?;
            debug!("Finished processing sync event.");
        }
        unreachable!("Fetching data loop should never return.");
    }

    // Tries to store the incoming data.
    async fn process_sync_event(&mut self, sync_event: SyncEvent) -> StateSyncResult {
        match sync_event {
            SyncEvent::BlockAvailable { block_number, block } => {
                debug!("Got block sync event.");
                self.store_block(block_number, block)
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            } => {
                debug!("Got state diff sync event.");
                self.store_state_diff(
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                )
            }
        }
    }

    fn store_block(&mut self, block_number: BlockNumber, block: Block) -> StateSyncResult {
        // Assuming the central source is trusted, detect reverts by comparing the incoming block's
        // parent hash to the current hash.
        self.verify_parent_block_hash(block_number, &block)?;

        debug!("Storing block {block_number} with hash {}.", block.header.block_hash);
        trace!("Block data: {block:#?}");
        self.writer
            .begin_rw_txn()?
            .append_header(block_number, &block.header)?
            .append_body(block_number, block.body)?
            .commit()?;
        Ok(())
    }

    fn store_state_diff(
        &mut self,
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
    ) -> StateSyncResult {
        if !self.is_reverted_state_diff(block_number, block_hash)? {
            debug!("Storing state diff of block {block_number} with hash {block_hash}.");
            trace!("StateDiff data: {state_diff:#?}");
            self.writer
                .begin_rw_txn()?
                .append_state_diff(block_number, state_diff, deployed_contract_class_definitions)?
                .commit()?;

            // Info the user on syncing the block once all the data is stored.
            info!("Added block {} with hash {}.", block_number, block_hash);
        } else {
            debug!(
                "Storing ommer state diff of block {} with hash {:?}.",
                block_number, block_hash
            );
            self.writer
                .begin_rw_txn()?
                .insert_ommer_state_diff(
                    block_hash,
                    &state_diff.into(),
                    &deployed_contract_class_definitions,
                )?
                .commit()?;
        }
        Ok(())
    }

    // Compares the block's parent hash to the stored block.
    fn verify_parent_block_hash(
        &self,
        block_number: BlockNumber,
        block: &Block,
    ) -> StateSyncResult {
        let prev_block_number = match block_number.prev() {
            None => return Ok(()),
            Some(bn) => bn,
        };
        let prev_hash = self
            .reader
            .begin_ro_txn()?
            .get_block_header(prev_block_number)?
            .ok_or(StorageError::DBInconsistency {
                msg: format!(
                    "Missing block {prev_block_number} in the storage (for verifying block \
                     {block_number}).",
                ),
            })?
            .block_hash;

        if prev_hash != block.header.parent_hash {
            return Err(StateSyncError::ParentBlockHashMismatch {
                block_number,
                expected_parent_block_hash: block.header.parent_hash,
                stored_parent_block_hash: prev_hash,
            });
        }

        Ok(())
    }

    // Reverts data if needed.
    async fn handle_block_reverts(&mut self) -> Result<(), StateSyncError> {
        debug!("Handling block reverts.");
        let header_marker = self.reader.begin_ro_txn()?.get_header_marker()?;

        // Revert last blocks if needed.
        let mut last_block_in_storage = header_marker.prev();
        while let Some(block_number) = last_block_in_storage {
            if self.should_revert_block(block_number).await? {
                info!("Reverting block {}.", block_number);
                self.revert_block(block_number)?;
                last_block_in_storage = block_number.prev();
            } else {
                break;
            }
        }
        Ok(())
    }

    // Deletes the block data from the storage, moving it to the ommer tables.
    #[allow(clippy::expect_fun_call)]
    fn revert_block(&mut self, block_number: BlockNumber) -> StateSyncResult {
        let mut txn = self.writer.begin_rw_txn()?;

        let res = txn.revert_header(block_number)?;
        txn = res.0;
        if let Some(header) = res.1 {
            txn = txn.insert_ommer_header(header.block_hash, &header)?;

            let res = txn.revert_body(block_number)?;
            txn = res.0;
            if let Some((transactions, transaction_outputs, events)) = res.1 {
                txn = txn.insert_ommer_body(
                    header.block_hash,
                    &transactions,
                    &transaction_outputs,
                    events.as_slice(),
                )?;
            }

            let res = txn.revert_state_diff(block_number)?;
            txn = res.0;
            if let Some((thin_state_diff, declared_classes)) = res.1 {
                txn = txn.insert_ommer_state_diff(
                    header.block_hash,
                    &thin_state_diff,
                    &declared_classes,
                )?;
            }
        }

        txn.commit()?;
        Ok(())
    }

    /// Checks if centrals block hash at the block number is different from ours (or doesn't exist).
    /// If so, a revert is required.
    async fn should_revert_block(&self, block_number: BlockNumber) -> Result<bool, StateSyncError> {
        if let Some(central_block_hash) = self.central_source.get_block_hash(block_number).await? {
            let storage_block_header =
                self.reader.begin_ro_txn()?.get_block_header(block_number)?;

            match storage_block_header {
                Some(block_header) => Ok(block_header.block_hash != central_block_hash),
                None => Ok(false),
            }
        } else {
            // Block number doesn't exist in central, revert.
            Ok(true)
        }
    }

    fn is_reverted_state_diff(
        &self,
        block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> Result<bool, StateSyncError> {
        let txn = self.reader.begin_ro_txn()?;
        let storage_header = txn.get_block_header(block_number)?;
        match storage_header {
            Some(storage_header) if storage_header.block_hash == block_hash => Ok(false),
            _ => {
                // No matching header, check in the ommer headers.
                match txn.get_ommer_header(block_hash)? {
                    Some(_) => Ok(true),
                    None => Err(StateSyncError::StateDiffWithoutMatchingHeader {
                        block_number,
                        block_hash,
                    }),
                }
            }
        }
    }
}

fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
            let last_block_number = central_source.get_block_marker().await?;
            if header_marker == last_block_number {
                debug!("Waiting for more blocks.");
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
            debug!("Downloading blocks [{} - {}).", header_marker, last_block_number);
            let block_stream =
                central_source.stream_new_blocks(header_marker, last_block_number).fuse();
            pin_mut!(block_stream);
            while let Some(maybe_block) = block_stream.next().await {
                let (block_number, block) = maybe_block?;
                yield SyncEvent::BlockAvailable { block_number, block };
            }
        }
    }
}

fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let state_marker = txn.get_state_marker()?;
            let last_block_number = txn.get_header_marker()?;
            drop(txn);
            if state_marker == last_block_number {
                debug!("Waiting for the block chain to advance.");
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
            debug!("Downloading state diffs [{} - {}).", state_marker, last_block_number);
            let state_diff_stream =
                central_source.stream_state_updates(state_marker, last_block_number).fuse();
            pin_mut!(state_diff_stream);

            while let Some(maybe_state_diff) = state_diff_stream.next().await {
                let (
                    block_number,
                    block_hash,
                    mut state_diff,
                    deployed_contract_class_definitions,
                ) = maybe_state_diff?;
                sort_state_diff(&mut state_diff);
                yield SyncEvent::StateDiffAvailable {
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                };
            }
        }
    }
}

pub fn sort_state_diff(diff: &mut StateDiff) {
    diff.declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
    }
}

pub type StateSync = GenericStateSync<CentralSource>;

impl StateSync {
    pub fn new(
        config: SyncConfig,
        central_source: CentralSource,
        reader: StorageReader,
        writer: StorageWriter,
    ) -> Self {
        Self { config, central_source: Arc::new(central_source), reader, writer }
    }
}
