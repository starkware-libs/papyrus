use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use futures::pin_mut;
use futures::stream::StreamExt;
use indexmap::IndexMap;
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio::sync::mpsc;
use tracing::{debug, trace};

use crate::sources::{CentralError, CentralSourceTrait};

#[derive(Debug, Clone)]
pub struct BlockSyncData {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct StateDiffSyncData {
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub state_diff: StateDiff,
    // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
    // Class definitions of deployed contracts with classes that were not declared in this
    // state diff.
    pub deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SyncData {
    Block(BlockSyncData),
    StateDiff(StateDiffSyncData),
}

#[async_trait]
pub trait SyncDataTrait: Sized + Sync + Send + Debug {
    fn r#type() -> SyncDataType;
    fn block_number(&self) -> BlockNumber;
    fn block_hash(&self) -> BlockHash;
    fn try_from(data: SyncData) -> Result<Self, SyncDataError>;
    async fn download<T: CentralSourceTrait + Sync + Send + 'static>(
        source: Arc<T>,
        sender: mpsc::Sender<SyncData>,
        from: BlockNumber,
        upto: BlockNumber,
    ) -> Result<(), SyncDataError>;
}

#[async_trait]
impl SyncDataTrait for BlockSyncData {
    fn r#type() -> SyncDataType {
        SyncDataType::Block
    }

    fn block_number(&self) -> BlockNumber {
        self.block.header.block_number
    }

    fn block_hash(&self) -> BlockHash {
        self.block.header.block_hash
    }

    fn try_from(data: SyncData) -> Result<Self, SyncDataError> {
        if let SyncData::Block(block_sync_data) = data {
            return Ok(block_sync_data);
        }
        Err(SyncDataError::DataConversion { msg: String::from("Expected block sync data type.") })
    }

    async fn download<T: CentralSourceTrait + Sync + Send + 'static>(
        source: Arc<T>,
        sender: mpsc::Sender<SyncData>,
        from: BlockNumber,
        upto: BlockNumber,
    ) -> Result<(), SyncDataError> {
        debug!("Downloading blocks [{}, {}).", from, upto);
        let block_stream = source.stream_new_blocks(from, upto).fuse();
        pin_mut!(block_stream);

        while let Some(maybe_block) = block_stream.next().await {
            let (_block_number, block) = maybe_block?;
            let block_number = block.header.block_number;
            sender.send(SyncData::Block(BlockSyncData { block })).await.map_err(|e| {
                SyncDataError::Channel {
                    msg: format!(
                        "Problem with sending block {block_number} when downloading [{from}, \
                         {upto}): {e}."
                    ),
                }
            })?;
            trace!("Downloaded block {block_number}.");
        }

        Ok(())
    }
}

#[async_trait]
impl SyncDataTrait for StateDiffSyncData {
    fn r#type() -> SyncDataType {
        SyncDataType::StateDiff
    }

    fn block_number(&self) -> BlockNumber {
        self.block_number
    }

    fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    fn try_from(data: SyncData) -> Result<Self, SyncDataError> {
        if let SyncData::StateDiff(state_diff_sync_data) = data {
            return Ok(state_diff_sync_data);
        }
        Err(SyncDataError::DataConversion {
            msg: String::from("Expected state diff sync data type."),
        })
    }

    async fn download<T: CentralSourceTrait + Sync + Send + 'static>(
        source: Arc<T>,
        sender: mpsc::Sender<SyncData>,
        from: BlockNumber,
        upto: BlockNumber,
    ) -> Result<(), SyncDataError> {
        debug!("Downloading state diffs [{}, {}).", from, upto);
        let state_diff_stream = source.stream_state_updates(from, upto).fuse();
        pin_mut!(state_diff_stream);

        while let Some(maybe_state_diff) = state_diff_stream.next().await {
            let (block_number, block_hash, state_diff, deployed_contract_class_definitions) =
                maybe_state_diff?;
            sender
                .send(SyncData::StateDiff(StateDiffSyncData {
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                }))
                .await
                .map_err(|e| SyncDataError::Channel {
                    msg: format!(
                        "Problem with sending state diff of block {block_number} when downloading \
                         [{from}, {upto}): {e}."
                    ),
                })?;
            trace!("Downloaded state diff of block {block_number}.");
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum SyncDataType {
    Block,
    StateDiff,
}

#[derive(thiserror::Error, Debug)]
pub enum SyncDataError {
    #[error(transparent)]
    CentralSource(#[from] CentralError),
    #[error("Channel error - {msg}")]
    Channel { msg: String },
    #[error("Data conversion error - {msg}")]
    DataConversion { msg: String },
}
