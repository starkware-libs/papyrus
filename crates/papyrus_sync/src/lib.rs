// config compiler to support no_coverage feature when running coverage in nightly mode within this
// crate
#![cfg_attr(coverage_nightly, feature(no_coverage))]

#[cfg(test)]
mod sync_test;

pub mod sources;

use std::cmp::min;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use chrono::{TimeZone, Utc};
use futures_util::{pin_mut, Stream, StreamExt};
use indexmap::IndexMap;
use papyrus_common::{metrics as papyrus_metrics, BlockHashAndNumber};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::{CasmStorageReader, CasmStorageWriter};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter, StarknetVersion};
use papyrus_storage::mmap_file::LocationInFile;
use papyrus_storage::ommer::{OmmerStorageReader, OmmerStorageWriter};
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use sources::base_layer::BaseLayerSourceError;
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{StateDiff, ThinStateDiff};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::sources::base_layer::{BaseLayerSourceTrait, EthereumBaseLayerSource};
use crate::sources::central::{CentralError, CentralSource, CentralSourceTrait};

// Sleep duration, in seconds, between sync progress checks.
const SLEEP_TIME_SYNC_PROGRESS: u64 = 120;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub block_propagation_sleep_duration: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub base_layer_propagation_sleep_duration: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub recoverable_error_sleep_duration: Duration,
    pub blocks_max_stream_size: u32,
    pub state_updates_max_stream_size: u32,
}

impl SerializeConfig for SyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "block_propagation_sleep_duration",
                &self.block_propagation_sleep_duration.as_secs(),
                "Time in seconds before checking for a new block after the node is synchronized.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "base_layer_propagation_sleep_duration",
                &self.base_layer_propagation_sleep_duration.as_secs(),
                "Time in seconds to poll the base layer to get the latest proved block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "recoverable_error_sleep_duration",
                &self.recoverable_error_sleep_duration.as_secs(),
                "Waiting time in seconds before restarting synchronization after a recoverable \
                 error.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "blocks_max_stream_size",
                &self.blocks_max_stream_size,
                "Max amount of blocks to download in a stream.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "state_updates_max_stream_size",
                &self.state_updates_max_stream_size,
                "Max amount of state updates to download in a stream.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            block_propagation_sleep_duration: Duration::from_secs(2),
            base_layer_propagation_sleep_duration: Duration::from_secs(10),
            recoverable_error_sleep_duration: Duration::from_secs(3),
            blocks_max_stream_size: 1000,
            state_updates_max_stream_size: 1000,
        }
    }
}

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage and shared
// memory.
pub struct GenericStateSync<
    TCentralSource: CentralSourceTrait + Sync + Send,
    TBaseLayerSource: BaseLayerSourceTrait + Sync + Send,
> {
    config: SyncConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    central_source: Arc<TCentralSource>,
    base_layer_source: Arc<TBaseLayerSource>,
    reader: StorageReader,
    writer: StorageWriter,
}

pub type StateSyncResult = Result<(), StateSyncError>;

#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error("Sync stopped progress.")]
    NoProgress,
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
    #[error("Header for block {block_number} wasn't found when trying to store base layer block.")]
    BaseLayerBlockWithoutMatchingHeader { block_number: BlockNumber },
    #[error(transparent)]
    BaseLayerSourceError(#[from] BaseLayerSourceError),
    #[error(
        "For {block_number} base layer and l2 doesn't match. Base layer hash: {base_layer_hash}, \
         L2 hash: {l2_hash}."
    )]
    BaseLayerHashMismatch {
        block_number: BlockNumber,
        base_layer_hash: BlockHash,
        l2_hash: BlockHash,
    },
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SyncEvent {
    NoProgress,
    BlockAvailable {
        block_number: BlockNumber,
        block: Block,
        starknet_version: StarknetVersion,
    },
    StateDiffAvailable {
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
        // Class definitions of deployed contracts with classes that were not declared in this
        // state diff.
        // Note: Since 0.11 new classes can not be implicitly declared.
        deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
    },
    CompiledClassAvailable {
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
        compiled_class: CasmContractClass,
    },
    NewBaseLayerBlock {
        block_number: BlockNumber,
        block_hash: BlockHash,
    },
}

impl<
    TCentralSource: CentralSourceTrait + Sync + Send + 'static,
    TBaseLayerSource: BaseLayerSourceTrait + Sync + Send,
> GenericStateSync<TCentralSource, TBaseLayerSource>
{
    pub async fn run(&mut self) -> StateSyncResult {
        info!("State sync started.");
        loop {
            let monitoring_task =
                start_monitoring_sync_progress(self.central_source.clone(), self.reader.clone());
            let res = tokio::select! {
                res = self.sync_while_ok() => res,
                res = monitoring_task => res
            };

            match res {
                // A recoverable error occurred. Sleep and try syncing again.
                Err(err) if is_recoverable(&err) => {
                    warn!("Recoverable error encountered while syncing, error: {}", err);
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    continue;
                }
                // Unrecoverable errors.
                Err(err) => {
                    error!("Fatal error while syncing: {}", err);
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
                StateSyncError::NoProgress => true,
                StateSyncError::CentralSourceError(_) => true,
                StateSyncError::BaseLayerSourceError(_) => true,
                StateSyncError::StorageError(storage_err)
                    if matches!(storage_err, StorageError::InnerError(_)) =>
                {
                    true
                }
                StateSyncError::StateDiffWithoutMatchingHeader {
                    block_number: _,
                    block_hash: _,
                } => true,
                StateSyncError::ParentBlockHashMismatch {
                    block_number,
                    expected_parent_block_hash,
                    stored_parent_block_hash,
                } => {
                    // A revert detected, log and restart sync loop.
                    info!(
                        "Detected revert while processing block {}. Parent hash of the incoming \
                         block is {}, current block hash is {}.",
                        block_number, expected_parent_block_hash, stored_parent_block_hash
                    );
                    true
                }
                StateSyncError::BaseLayerHashMismatch { .. } => true,
                StateSyncError::BaseLayerBlockWithoutMatchingHeader { .. } => true,
                _ => false,
            }
        }
    }

    // Sync until encountering an error:
    //
    // If needed, revert blocks from the end of the chain.
    // In case there is no progress for a while, return an error.
    // loop:
    //  1. Fetch and store blocks.
    //  2. Check agains base layer and update finality if needed.
    //  3. Fetch and store state diffs.
    //  4. Fetch and store compiled classes.
    async fn sync_while_ok(&mut self) -> StateSyncResult {
        // self.start_monitoring_sync_progress();
        self.handle_block_reverts().await?;

        let mut consecutive_iterations = 0;
        loop {
            debug!("Syncing iteration {}.", consecutive_iterations);
            self.sync_blocks().await?;
            self.sync_base_layer_finality().await?;
            self.sync_state_diffs().await?;
            self.sync_compiled_classes().await?;
            consecutive_iterations += 1;
            debug!("Finished all stages {consecutive_iterations} consecutive times.");
            self.sleep_if_needed().await?;
        }
    }

    #[instrument(skip(self, blocks), level = "debug", fields(n_blocks), err)]
    fn store_blocks(
        &mut self,
        initial_block_number: Option<BlockNumber>,
        blocks: Vec<(Block, StarknetVersion)>,
    ) -> StateSyncResult {
        let n_blocks = blocks.len();
        tracing::Span::current().record("n_blocks", n_blocks);
        debug!("Storing blocks.");
        let Some(initial_block_number) = initial_block_number else {
            return Ok(());
        };

        let mut block_number = initial_block_number;
        let mut prev_block_hash = blocks.first().expect("should have a value").0.header.parent_hash;
        let last_block_timestamp = blocks.last().expect("should have a value").0.header.timestamp;

        let mut parent_block_mismatch_error = None;
        let mut headers = Vec::with_capacity(n_blocks);
        let mut bodies = Vec::with_capacity(n_blocks);
        let mut starknet_versions = Vec::with_capacity(n_blocks);

        for (block, starknet_version) in blocks {
            trace!("Block data: {block:#?}");
            if block.header.parent_hash != prev_block_hash {
                parent_block_mismatch_error = Some(StateSyncError::ParentBlockHashMismatch {
                    block_number,
                    expected_parent_block_hash: block.header.parent_hash,
                    stored_parent_block_hash: prev_block_hash,
                });
                break;
            }
            prev_block_hash = block.header.block_hash;
            block_number = block_number.next();

            headers.push(block.header);
            bodies.push(block.body);
            starknet_versions.push(starknet_version);
        }
        self.writer
            .begin_rw_txn()?
            .append_headers(initial_block_number, &headers)?
            .append_bodies(initial_block_number, bodies)?
            .update_starknet_versions(&initial_block_number, starknet_versions)?
            .commit()?;

        metrics::gauge!(papyrus_metrics::PAPYRUS_HEADER_MARKER, block_number.0 as f64);
        metrics::gauge!(papyrus_metrics::PAPYRUS_BODY_MARKER, block_number.0 as f64);
        let dt = Utc::now()
            - Utc
                .timestamp_opt(last_block_timestamp.0 as i64, 0)
                .single()
                .expect("block timestamp should be valid");
        let header_latency = dt.num_seconds();
        debug!("Header latency: {}.", header_latency);
        if header_latency >= 0 {
            metrics::gauge!(papyrus_metrics::PAPYRUS_HEADER_LATENCY_SEC, header_latency as f64);
        }

        if let Some(err) = parent_block_mismatch_error {
            return Err(err);
        }
        Ok(())
    }

    #[instrument(skip(self, state_diffs), level = "debug", fields(n_blocks), err)]
    fn store_state_diffs(
        &mut self,
        initial_block_number: Option<BlockNumber>,
        state_diffs: Vec<(BlockHash, StateDiff, IndexMap<ClassHash, DeprecatedContractClass>)>,
    ) -> StateSyncResult {
        let n_blocks = state_diffs.len();
        tracing::Span::current().record("n_blocks", n_blocks);
        debug!("Storing state diffs.");
        let Some(initial_block_number) = initial_block_number else {
            return Ok(());
        };

        let mut block_number = initial_block_number;
        let mut thin_state_diff_locations_to_write = Vec::with_capacity(n_blocks);
        let mut diffs = Vec::with_capacity(n_blocks);
        let mut offset = self.writer.get_thin_state_diff_offset();
        for (block_hash, state_diff, deployed_contract_class_definitions) in state_diffs {
            if !self.is_reverted_state_diff(block_number, block_hash)? {
                trace!("StateDiff data: {state_diff:#?}");

                let thin_state_diff_ref = thin_state_diff_from_state_diff_ref(&state_diff);
                let len = self.writer.insert_thin_state_diff(offset, &thin_state_diff_ref);
                // info!("Inserted thin state diff at offset {} of length {}.", offset, len);
                thin_state_diff_locations_to_write.push(LocationInFile { offset, len });
                offset += len;

                diffs.push((state_diff, deployed_contract_class_definitions));
            } else {
                debug!("TODO: Insert reverted state diff to ommer table.");
                break;
            }
            block_number = block_number.next();
        }

        self.writer.flush_thin_state_diff();
        self.writer
            .begin_rw_txn()?
            .append_state_diffs(initial_block_number, diffs, &thin_state_diff_locations_to_write)?
            .commit()?;
        metrics::gauge!(papyrus_metrics::PAPYRUS_STATE_MARKER, block_number.0 as f64);
        let compiled_class_marker = self.reader.begin_ro_txn()?.get_compiled_class_marker()?;
        metrics::gauge!(
            papyrus_metrics::PAPYRUS_COMPILED_CLASS_MARKER,
            compiled_class_marker.0 as f64
        );

        // Info the user on syncing the block once all the data is stored.
        debug!("Added state upto block {}.", block_number.prev().unwrap_or_default());
        Ok(())
    }

    #[instrument(skip(self, state_diff, deployed_contract_class_definitions), level = "debug", err)]
    fn store_state_diff(
        &mut self,
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        deployed_contract_class_definitions: IndexMap<ClassHash, DeprecatedContractClass>,
    ) -> StateSyncResult {
        if !self.is_reverted_state_diff(block_number, block_hash)? {
            debug!("Storing state diff.");
            trace!("StateDiff data: {state_diff:#?}");
            self.writer
                .begin_rw_txn()?
                .append_state_diff(block_number, state_diff, deployed_contract_class_definitions)?
                .commit()?;
            metrics::gauge!(papyrus_metrics::PAPYRUS_STATE_MARKER, block_number.next().0 as f64);
            let compiled_class_marker = self.reader.begin_ro_txn()?.get_compiled_class_marker()?;
            metrics::gauge!(
                papyrus_metrics::PAPYRUS_COMPILED_CLASS_MARKER,
                compiled_class_marker.0 as f64
            );

            // Info the user on syncing the block once all the data is stored.
            info!("Added block {} with hash {}.", block_number, block_hash);
        } else {
            debug!("TODO: Insert reverted state diff to ommer table.");
        }
        Ok(())
    }

    #[instrument(skip(self, compiled_class), level = "debug", err)]
    fn store_compiled_class(
        &mut self,
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
        compiled_class: CasmContractClass,
    ) -> StateSyncResult {
        let txn = self.writer.begin_rw_txn()?;
        let is_reverted_class =
            txn.get_state_reader()?.get_class_definition_block_number(&class_hash)?.is_none();
        if is_reverted_class {
            debug!("TODO: Insert reverted compiled class to ommer table.");
        }
        match txn.append_casm(&class_hash, &compiled_class) {
            Ok(txn) => {
                txn.commit()?;
                let compiled_class_marker =
                    self.reader.begin_ro_txn()?.get_compiled_class_marker()?;
                metrics::gauge!(
                    papyrus_metrics::PAPYRUS_COMPILED_CLASS_MARKER,
                    compiled_class_marker.0 as f64
                );
                debug!("Added compiled class.");
                Ok(())
            }
            // TODO(yair): Modify the stream so it skips already stored classes.
            // Compiled classes rewrite is valid because the stream downloads from the beginning of
            // the block instead of the last downloaded class.
            Err(StorageError::CompiledClassReWrite { class_hash: existing_class_hash })
                if existing_class_hash == class_hash =>
            {
                debug!("Compiled class of {class_hash} already stored.");
                Ok(())
            }
            Err(err) => Err(StateSyncError::StorageError(err)),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    // In case of a mismatch between the base layer and l2, an error will be returned, then the
    // sync will revert blocks if needed based on the l2 central source. This approach works as long
    // as l2 is trusted so all the reverts can be detect by using it.
    fn store_base_layer_block(
        &mut self,
        block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> StateSyncResult {
        let txn = self.writer.begin_rw_txn()?;
        // Missing header can be because of a base layer reorg, the matching header may be reverted.
        let expected_hash = txn
            .get_block_header(block_number)?
            .ok_or(StateSyncError::BaseLayerBlockWithoutMatchingHeader { block_number })?
            .block_hash;
        // Can be caused because base layer reorg or l2 reverts.
        if expected_hash != block_hash {
            return Err(StateSyncError::BaseLayerHashMismatch {
                block_number,
                base_layer_hash: block_hash,
                l2_hash: expected_hash,
            });
        }
        info!("Verified block {block_number} hash against base layer.");
        txn.update_base_layer_block_marker(&block_number.next())?.commit()?;
        metrics::gauge!(papyrus_metrics::PAPYRUS_BASE_LAYER_MARKER, block_number.next().0 as f64);
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
                self.revert_block(block_number)?;
                last_block_in_storage = block_number.prev();
            } else {
                break;
            }
        }
        Ok(())
    }

    // TODO(dan): update necessary metrics.
    // Deletes the block data from the storage, moving it to the ommer tables.
    #[allow(clippy::expect_fun_call)]
    #[instrument(skip(self), level = "debug", err)]
    fn revert_block(&mut self, block_number: BlockNumber) -> StateSyncResult {
        debug!("Reverting block.");
        let mut txn = self.writer.begin_rw_txn()?;

        txn = txn.try_revert_base_layer_marker(block_number)?;
        let res = txn.revert_header(block_number)?;
        txn = res.0;
        let mut reverted_block_hash: Option<BlockHash> = None;
        if let Some(header) = res.1 {
            reverted_block_hash = Some(header.block_hash);
            txn = txn.insert_ommer_header(header.block_hash, &header)?;

            let res = txn.revert_body(block_number)?;
            txn = res.0;
            if let Some((transactions, transaction_outputs, _transaction_hashes, events)) = res.1 {
                txn = txn.insert_ommer_body(
                    header.block_hash,
                    &transactions,
                    &transaction_outputs,
                    events.as_slice(),
                )?;
            }

            let res = txn.revert_state_diff(block_number)?;
            txn = res.0;
            // TODO(yair): consider inserting to ommer the deprecated_declared_classes.
            if let Some((
                thin_state_diff,
                declared_classes,
                _deprecated_declared_classes,
                _compiled_classes,
            )) = res.1
            {
                txn = txn.insert_ommer_state_diff(
                    header.block_hash,
                    &thin_state_diff,
                    &declared_classes,
                )?;
            }
        }

        txn.commit()?;
        if let Some(hash) = reverted_block_hash {
            info!(%hash, "Reverted block.");
        }
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

    async fn sync_blocks(&mut self) -> Result<(), StateSyncError> {
        debug!("Syncing blocks.");
        const MAX_BLOCKS_BEFORE_PERSISTING: usize = 500;
        let block_stream = stream_new_blocks(
            self.reader.clone(),
            self.central_source.clone(),
            self.shared_highest_block.clone(),
            self.config.blocks_max_stream_size,
        )
        .fuse();
        pin_mut!(block_stream);
        let mut initial_block_number: Option<BlockNumber> = None;
        let mut blocks: Vec<(Block, StarknetVersion)> =
            Vec::with_capacity(MAX_BLOCKS_BEFORE_PERSISTING);
        while let Some(maybe_block) = block_stream.next().await {
            let SyncEvent::BlockAvailable { block_number, block, starknet_version } = maybe_block?
            else {
                panic!("Expected block.")
            };
            if initial_block_number.is_none() {
                // Assuming the central source is trusted, detect reverts by comparing the incoming
                // block's parent hash to the current hash.
                self.verify_parent_block_hash(block_number, &block)?;
                initial_block_number = Some(block_number);
            }
            blocks.push((block, starknet_version));
            if blocks.len() >= MAX_BLOCKS_BEFORE_PERSISTING {
                self.store_blocks(initial_block_number, blocks)?;
                blocks = Vec::with_capacity(MAX_BLOCKS_BEFORE_PERSISTING);
                initial_block_number = None;
            }
        }
        self.store_blocks(initial_block_number, blocks)?;
        Ok(())
    }

    async fn sync_base_layer_finality(&mut self) -> Result<(), StateSyncError> {
        debug!("Syncing base layer finality.");
        let header_marker = self.reader.begin_ro_txn()?.get_header_marker()?;
        match self.base_layer_source.latest_proved_block().await? {
            Some((block_number, _block_hash)) if header_marker <= block_number => {
                debug!(
                    "Sync headers ({header_marker}) is behind the base layer tip \
                     ({block_number}), waiting for sync to advance."
                );
            }
            Some((block_number, block_hash)) => {
                debug!("Returns a block from the base layer. Block number: {block_number}.");
                self.store_base_layer_block(block_number, block_hash)?;
            }
            None => {
                debug!("No blocks were proved on the base layer.");
            }
        }
        Ok(())
    }

    async fn sync_state_diffs(&mut self) -> Result<(), StateSyncError> {
        debug!("Syncing state diffs.");
        const MAX_STATE_DIFF_BEFORE_PERSISTING: usize = 50;
        let state_diff_stream = stream_new_state_diffs(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.state_updates_max_stream_size,
        )
        .fuse();
        pin_mut!(state_diff_stream);
        let mut initial_block_number: Option<BlockNumber> = None;
        let mut state_diffs: Vec<(
            BlockHash,
            StateDiff,
            IndexMap<ClassHash, DeprecatedContractClass>,
        )> = Vec::with_capacity(MAX_STATE_DIFF_BEFORE_PERSISTING);
        while let Some(maybe_state_diff) = state_diff_stream.next().await {
            let SyncEvent::StateDiffAvailable {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            } = maybe_state_diff?
            else {
                panic!("Expected state diff.")
            };
            if initial_block_number.is_none() {
                initial_block_number = Some(block_number);
            }
            state_diffs.push((block_hash, state_diff, deployed_contract_class_definitions));
            if state_diffs.len() >= MAX_STATE_DIFF_BEFORE_PERSISTING {
                self.store_state_diffs(initial_block_number, state_diffs)?;
                state_diffs = Vec::with_capacity(MAX_STATE_DIFF_BEFORE_PERSISTING);
                initial_block_number = None;
            }
        }
        self.store_state_diffs(initial_block_number, state_diffs)?;
        Ok(())
    }

    async fn sync_compiled_classes(&mut self) -> Result<(), StateSyncError> {
        debug!("Syncing compiled classes.");
        let compiled_class_stream = stream_new_compiled_classes(
            self.reader.clone(),
            self.central_source.clone(),
            // TODO(yair): separate config param.
            self.config.state_updates_max_stream_size,
        )
        .fuse();
        pin_mut!(compiled_class_stream);
        while let Some(maybe_compiled_class) = compiled_class_stream.next().await {
            let SyncEvent::CompiledClassAvailable {
                class_hash,
                compiled_class_hash,
                compiled_class,
            } = maybe_compiled_class?
            else {
                panic!("Expected compiled class.")
            };
            self.store_compiled_class(class_hash, compiled_class_hash, compiled_class)?;
        }
        Ok(())
    }

    // Assuming this is called after the last stage of the sync.
    // If the header marker is equal to the central block marker, sleep for a while.
    async fn sleep_if_needed(&self) -> Result<(), StateSyncError> {
        let header_marker = self.reader.begin_ro_txn()?.get_header_marker()?;
        let latest_central_block = self.central_source.get_latest_block().await?;
        *self.shared_highest_block.write().await = latest_central_block;
        let central_block_marker =
            latest_central_block.map_or(BlockNumber::default(), |block| block.block_number.next());
        metrics::gauge!(
            papyrus_metrics::PAPYRUS_CENTRAL_BLOCK_MARKER,
            central_block_marker.0 as f64
        );
        if header_marker == central_block_marker {
            debug!("Syncing reached the last known block, waiting for blockchain to advance.");
            tokio::time::sleep(self.config.block_propagation_sleep_duration).await;
        }
        Ok(())
    }
}
async fn start_monitoring_sync_progress<
    TCentralSource: CentralSourceTrait + Sync + Send + 'static,
>(
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
) -> Result<(), StateSyncError> {
    let central_source = central_source.clone();
    let reader = reader.clone();

    let join_handle = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(SLEEP_TIME_SYNC_PROGRESS));
        interval.tick().await;

        let latest_central_block = central_source.get_latest_block().await?;
        let mut central_block_marker =
            latest_central_block.map_or(BlockNumber::default(), |block| block.block_number.next());
        let mut txn = reader.begin_ro_txn()?;
        let mut header_marker = txn.get_header_marker()?;
        let mut state_marker = txn.get_state_marker()?;
        let mut casm_marker = txn.get_compiled_class_marker()?;
        loop {
            interval.tick().await;
            let new_latest_central_block = central_source.get_latest_block().await?;
            let new_central_block_marker = new_latest_central_block
                .map_or(BlockNumber::default(), |block| block.block_number.next());
            txn = reader.begin_ro_txn()?;
            let new_header_marker = txn.get_header_marker()?;
            let new_state_marker = txn.get_state_marker()?;
            let new_casm_marker = txn.get_compiled_class_marker()?;
            let central_block_progress = new_central_block_marker.0 - central_block_marker.0;
            debug!(
                "Checking if sync stopped progress. Central progress: {}-{}. Header progress: \
                 {}-{}. State progress: {}-{}. CASM progress: {}-{}.",
                central_block_marker.0,
                new_central_block_marker.0,
                header_marker.0,
                new_header_marker.0,
                state_marker.0,
                new_state_marker.0,
                casm_marker.0,
                new_casm_marker.0,
            );
            if central_block_progress > 0
                && new_header_marker.0 - header_marker.0 <= 1
                && new_state_marker.0 - state_marker.0 <= 1
                && new_casm_marker.0 - casm_marker.0 <= 1
            {
                error!("No progress in the sync for {SLEEP_TIME_SYNC_PROGRESS} seconds.");
                return Err(StateSyncError::NoProgress);
            }
            central_block_marker = new_central_block_marker;
            header_marker = new_header_marker;
            state_marker = new_state_marker;
            casm_marker = new_casm_marker;
        }
    });
    join_handle.await.expect("Sync progress monitoring thread panicked.")
}

fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    max_stream_size: u32,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        let mut next_block_number = reader.begin_ro_txn()?.get_header_marker()?;
        loop {
            let latest_central_block = Some(
                BlockHashAndNumber {
                    block_hash: BlockHash::default(),
                    block_number: BlockNumber(200000)
                }
            );
            // let latest_central_block = central_source.get_latest_block().await?;
            *shared_highest_block.write().await = latest_central_block;
            let central_block_marker = latest_central_block.map_or(
                BlockNumber::default(), |block| block.block_number.next()
            );
            metrics::gauge!(
                papyrus_metrics::PAPYRUS_CENTRAL_BLOCK_MARKER, central_block_marker.0 as f64
            );
            if next_block_number == central_block_marker {
                debug!("Blocks syncing reached the last known block, waiting for blockchain to advance.");
                return;
            }
            let up_to = min(central_block_marker, BlockNumber(next_block_number.0 + max_stream_size as u64));
            debug!("Downloading blocks [{next_block_number} - {up_to}/{central_block_marker}).");
            let block_stream =
                central_source.stream_new_blocks(next_block_number, up_to).fuse();
            pin_mut!(block_stream);
            while let Some(maybe_block) = block_stream.next().await {
                let (block_number, block, starknet_version) = maybe_block?;
                yield SyncEvent::BlockAvailable { block_number, block , starknet_version};
                next_block_number = next_block_number.next();
            }
        }
    }
}

fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    max_stream_size: u32,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let state_marker = txn.get_state_marker()?;
            let last_block_number = txn.get_header_marker()?;
            drop(txn);
            if state_marker == last_block_number {
                debug!("State updates syncing reached the last downloaded block, waiting for more blocks.");
                return;
            }
            let up_to = min(last_block_number, BlockNumber(state_marker.0 + max_stream_size as u64));
            debug!("Downloading state diffs [{state_marker} - {up_to}/{last_block_number}).");
            let state_diff_stream =
                central_source.stream_state_updates(state_marker, up_to).fuse();
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
    diff.deprecated_declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.replaced_classes.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
    }
}

pub type StateSync = GenericStateSync<CentralSource, EthereumBaseLayerSource>;

impl StateSync {
    pub fn new(
        config: SyncConfig,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        central_source: CentralSource,
        base_layer_source: EthereumBaseLayerSource,
        reader: StorageReader,
        writer: StorageWriter,
    ) -> Self {
        Self {
            config,
            shared_highest_block,
            central_source: Arc::new(central_source),
            base_layer_source: Arc::new(base_layer_source),
            reader,
            writer,
        }
    }
}

fn stream_new_compiled_classes<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    max_stream_size: u32,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let mut from = txn.get_compiled_class_marker()?;
            let state_marker = txn.get_state_marker()?;
            // Avoid starting streams from blocks without declared classes.
            while from < state_marker {
                let state_diff = txn.get_state_diff(from)?.expect("Expecting to have state diff up to the marker.");
                if state_diff.declared_classes.is_empty() {
                    from = from.next();
                }
                else {
                    break;
                }
            }

            if from == state_marker {
                debug!(
                    "Compiled classes syncing reached the last downloaded state update, waiting \
                     for more state updates."
                );
                return ;

            }
            let up_to = min(state_marker, BlockNumber(from.0 + max_stream_size as u64));
            debug!("Downloading compiled classes of blocks [{from} - {up_to}/{state_marker}).");
            let compiled_classes_stream =
                central_source.stream_compiled_classes(from, up_to).fuse();
            pin_mut!(compiled_classes_stream);

            // TODO(yair): Consider adding the block number and hash in order to make sure
            // that we do not write classes of ommer blocks.
            while let Some(maybe_compiled_class) = compiled_classes_stream.next().await {
                let (class_hash, compiled_class_hash, compiled_class) = maybe_compiled_class?;
                yield SyncEvent::CompiledClassAvailable {
                    class_hash,
                    compiled_class_hash,
                    compiled_class,
                };
            }
        }
    }
}

// Returns a reference to the thin state diff from a StateDiff.
pub fn thin_state_diff_from_state_diff_ref(diff: &StateDiff) -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: diff.deployed_contracts.clone(),
        storage_diffs: diff.storage_diffs.clone(),
        declared_classes: diff
            .declared_classes
            .iter()
            .map(|(class_hash, (compiled_hash, _class))| (*class_hash, *compiled_hash))
            .collect(),
        deprecated_declared_classes: diff.deprecated_declared_classes.keys().copied().collect(),
        nonces: diff.nonces.clone(),
        replaced_classes: diff.replaced_classes.clone(),
    }
}
