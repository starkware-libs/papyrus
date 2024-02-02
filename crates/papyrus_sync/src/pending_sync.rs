use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::FuturesUnordered;
use futures_util::{FutureExt, StreamExt};
use papyrus_common::pending_classes::{PendingClasses, PendingClassesTrait};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
// TODO(shahak): Consider adding genesis hash to the config to support chains that have
// different genesis hash.
use starknet_api::hash::GENESIS_HASH;
use starknet_client::reader::{DeclaredClassHashEntry, PendingData};
use starknet_types_core::felt::Felt;
use tokio::sync::RwLock;
use tracing::{debug, trace};

use crate::sources::central::CentralSourceTrait;
use crate::sources::pending::PendingSourceTrait;
use crate::StateSyncError;

// Update the pending data and return when a new block is discovered.
pub(crate) async fn sync_pending_data<
    TPendingSource: PendingSourceTrait + Sync + Send + 'static,
    TCentralSource: CentralSourceTrait + Sync + Send + 'static,
>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    pending_source: Arc<TPendingSource>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    sleep_duration: Duration,
) -> Result<(), StateSyncError> {
    let txn = reader.begin_ro_txn()?;
    let header_marker = txn.get_header_marker()?;
    // TODO: Consider extracting this functionality to different а function.
    let latest_block_hash = match header_marker {
        BlockNumber(0) => BlockHash(Felt::from(GENESIS_HASH)),
        _ => {
            txn.get_block_header(
                header_marker
                    .prev()
                    .expect("All blocks other than the first block should have a predecessor."),
            )?
            .expect("Block before the header marker must have header in the database.")
            .block_hash
        }
    };
    let mut tasks = FuturesUnordered::new();
    tasks.push(
        get_pending_data(
            latest_block_hash,
            pending_source.clone(),
            pending_data.clone(),
            pending_classes.clone(),
            Duration::ZERO,
        )
        .boxed(),
    );
    let mut processed_classes = HashSet::new();
    let mut processed_compiled_classes = HashSet::new();
    loop {
        match tasks.next().await.expect("There should always be a task in the pending sync")? {
            PendingSyncTaskResult::PendingSyncFinished => return Ok(()),
            PendingSyncTaskResult::DownloadedNewPendingData => {
                let (declared_classes, old_declared_contracts) = {
                    // TODO (shahak): Consider getting the pending data from the task result instead
                    // of reading from the lock.
                    let pending_state_diff = &pending_data.read().await.state_update.state_diff;
                    (
                        pending_state_diff.declared_classes.clone(),
                        pending_state_diff.old_declared_contracts.clone(),
                    )
                };
                for DeclaredClassHashEntry { class_hash, compiled_class_hash } in declared_classes {
                    if processed_classes.insert(class_hash) {
                        tasks.push(
                            get_pending_class(
                                class_hash,
                                central_source.clone(),
                                pending_classes.clone(),
                            )
                            .boxed(),
                        );
                    }
                    if processed_compiled_classes.insert(compiled_class_hash) {
                        tasks.push(
                            get_pending_compiled_class(
                                class_hash,
                                central_source.clone(),
                                pending_classes.clone(),
                            )
                            .boxed(),
                        );
                    }
                }
                for class_hash in old_declared_contracts {
                    if processed_classes.insert(class_hash) {
                        tasks.push(
                            get_pending_class(
                                class_hash,
                                central_source.clone(),
                                pending_classes.clone(),
                            )
                            .boxed(),
                        );
                    }
                }
                tasks.push(
                    get_pending_data(
                        latest_block_hash,
                        pending_source.clone(),
                        pending_data.clone(),
                        pending_classes.clone(),
                        sleep_duration,
                    )
                    .boxed(),
                )
            }
            PendingSyncTaskResult::DownloadedOldPendingData => tasks.push(
                get_pending_data(
                    latest_block_hash,
                    pending_source.clone(),
                    pending_data.clone(),
                    pending_classes.clone(),
                    sleep_duration,
                )
                .boxed(),
            ),
            PendingSyncTaskResult::DownloadedClassOrCompiledClass => {}
        }
    }
}

enum PendingSyncTaskResult {
    DownloadedNewPendingData,
    DownloadedOldPendingData,
    PendingSyncFinished,
    DownloadedClassOrCompiledClass,
}

async fn get_pending_data<TPendingSource: PendingSourceTrait + Sync + Send + 'static>(
    latest_block_hash: BlockHash,
    pending_source: Arc<TPendingSource>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    sleep_duration: Duration,
) -> Result<PendingSyncTaskResult, StateSyncError> {
    tokio::time::sleep(sleep_duration).await;

    let new_pending_data = pending_source.get_pending_data().await?;

    // In Starknet, if there's no pending block then the latest block is returned. We prefer to
    // treat this case as if the pending block is an empty block on top of the latest block.
    // We distinguish this case by looking if the block_hash field is present.
    let new_pending_parent_hash =
        new_pending_data.block.block_hash.unwrap_or(new_pending_data.block.parent_block_hash);
    if new_pending_parent_hash != latest_block_hash {
        // TODO(shahak): If block_hash is present, consider writing the pending data here so that
        // the pending data will be available until the node syncs on the new block.
        debug!("A new block was found. Stopping pending sync.");
        return Ok(PendingSyncTaskResult::PendingSyncFinished);
    };

    let (current_pending_num_transactions, current_pending_parent_hash) = {
        let pending_block = &pending_data.read().await.block;
        (
            pending_block.transactions.len(),
            pending_block.block_hash.unwrap_or(pending_block.parent_block_hash),
        )
    };
    let is_new_pending_data_more_advanced = current_pending_parent_hash != new_pending_parent_hash
        || new_pending_data.block.transactions.len() > current_pending_num_transactions;
    if is_new_pending_data_more_advanced {
        debug!("Received new pending data.");
        trace!("Pending data: {new_pending_data:#?}.");
        if current_pending_parent_hash != new_pending_parent_hash {
            pending_classes.write().await.clear();
        }
        *pending_data.write().await = new_pending_data;
        Ok(PendingSyncTaskResult::DownloadedNewPendingData)
    } else {
        debug!("Pending block wasn't updated. Waiting for pending block to be updated.");
        Ok(PendingSyncTaskResult::DownloadedOldPendingData)
    }
}

async fn get_pending_class<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    class_hash: ClassHash,
    central_source: Arc<TCentralSource>,
    pending_classes: Arc<RwLock<PendingClasses>>,
) -> Result<PendingSyncTaskResult, StateSyncError> {
    let class = central_source.get_class(class_hash).await?;
    pending_classes.write().await.add_class(class_hash, class);
    Ok(PendingSyncTaskResult::DownloadedClassOrCompiledClass)
}

async fn get_pending_compiled_class<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    class_hash: ClassHash,
    central_source: Arc<TCentralSource>,
    pending_classes: Arc<RwLock<PendingClasses>>,
) -> Result<PendingSyncTaskResult, StateSyncError> {
    let compiled_class = central_source.get_compiled_class(class_hash).await?;
    pending_classes.write().await.add_compiled_class(class_hash, compiled_class);
    Ok(PendingSyncTaskResult::DownloadedClassOrCompiledClass)
}
