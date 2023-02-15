pub mod header;
pub mod state;

mod sources;

use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio_stream::StreamExt;
use tracing::{debug, error, info};

use crate::header::{process_block_event, sync_block_while_ok};
pub use crate::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};
use crate::state::{process_state_diff_event, sync_state_while_ok};

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
    #[error(transparent)]
    TokioJoinError(#[from] tokio::task::JoinError),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SyncEvent {
    BlockAvailable {
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
    pub async fn run(&mut self) {
        info!("State sync started.");

        let mut state_sync_idle = false;
        loop {
            let block_sync_task = sync_block_while_ok(
                self.reader.clone(),
                self.central_source.clone(),
                self.config.block_propagation_sleep_duration,
            )
            .fuse();
            let state_sync_task = sync_state_while_ok(
                self.reader.clone(),
                self.central_source.clone(),
                self.config.block_propagation_sleep_duration,
                state_sync_idle,
            )
            .fuse();
            tokio::pin!(block_sync_task, state_sync_task);

            loop {
                debug!("Selecting between blocks and state sync.");
                let res = tokio::select! {
                    res = block_sync_task.next() => res,
                    res = state_sync_task.next() => res,
                }
                .expect("Received None as a sync event.");
                if let Err(err) = res {
                    error!("{}", err);
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    break;
                }

                let res = self.process_sync_event(res.unwrap());
                if let Err(err) = res {
                    error!("{}", err);
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    break;
                }
                state_sync_idle = res.as_ref().unwrap().1;
                if res.unwrap().0 || state_sync_idle {
                    break;
                }
            }
        }
    }

    fn process_sync_event(
        &mut self,
        sync_event: SyncEvent,
    ) -> Result<(bool, bool), StateSyncError> {
        let txn = self.writer.begin_rw_txn()?;
        match sync_event {
            SyncEvent::BlockAvailable { block } => {
                debug!("Got block sync event.");
                let (txn, revert_happened) = process_block_event(txn, self.reader.clone(), block)?;
                txn.commit()?;
                debug!(
                    "Done processing block sync event: revert_happened {revert_happened}, \
                     state_sync_idle false"
                );
                Ok((revert_happened, false))
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            } => {
                debug!("Got state diff sync event.");
                let (txn, state_sync_idle) = process_state_diff_event(
                    txn,
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                )?;
                txn.commit()?;
                debug!(
                    "Done processing state diff sync event: revert_happened false, \
                     state_sync_idle {state_sync_idle}"
                );
                Ok((false, state_sync_idle))
            }
        }
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
