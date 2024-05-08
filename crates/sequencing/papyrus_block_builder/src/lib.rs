//! This crate contains a mock block-builder that echoes [`Starknet`] blocks.
//!
//!
//! [`Starknet`]: https://starknet.io/

use std::sync::mpsc::{self, Receiver};

use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::Transaction;
use tracing::instrument;

#[cfg(test)]
mod test;

/// A block builder.
struct BlockBuilder {
    // A storage reader to read blocks from. Will be replaced with mempool.
    #[allow(unused)]
    storage_reader: StorageReader,
}

pub trait BlockBuilderTrait {
    fn build(&self, block_number: BlockNumber) -> BlockBuilderResult<Receiver<Transaction>>;
}

type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

#[derive(thiserror::Error, Debug)]
pub enum BlockBuilderError {
    #[error("Could not find a block with block number {}.", block_number)]
    BlockNotFound { block_number: BlockNumber },
}

impl BlockBuilder {
    /// Create a new block builder.
    #[allow(unused)]
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { storage_reader }
    }
}

impl BlockBuilderTrait for BlockBuilder {
    #[instrument(skip(self), level = "debug")]
    fn build(&self, block_number: BlockNumber) -> BlockBuilderResult<Receiver<Transaction>> {
        let (sender, receiver) = mpsc::channel();

        // TODO: spawn a task to send the transactions and return the receiver immediately.
        let block = self
            .storage_reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transactions(block_number)
            .unwrap();

        match block {
            Some(block) => {
                for txn in block {
                    sender.send(txn).unwrap();
                }
                Ok(receiver)
            }
            None => Err(BlockBuilderError::BlockNotFound { block_number }),
        }
    }
}
