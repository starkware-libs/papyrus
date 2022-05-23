use async_trait::async_trait;

use crate::starknet::{BlockBody, BlockHash, BlockHeader};

pub struct StorageError {}

#[async_trait]
pub trait StorageHandle: Clone {
    async fn add_block_header(&self, _block_header: BlockHeader)
        -> Result<BlockHash, StorageError>;
    async fn add_block_body(
        &self,
        _block_id: BlockHash,
        _block_body: BlockBody,
    ) -> Result<(), StorageError>;
    async fn get_block_header(&self, _block_id: BlockHash) -> Result<BlockHeader, StorageError>;
}
