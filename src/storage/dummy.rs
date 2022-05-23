use async_trait::async_trait;

use crate::starknet::{BlockBody, BlockHash, BlockHeader};

use super::api::{StorageError, StorageHandle};

#[derive(Clone)]
pub struct DummyStorageHandleImpl {}

#[async_trait]
impl StorageHandle for DummyStorageHandleImpl {
    async fn add_block_header(
        &self,
        _block_header: BlockHeader,
    ) -> Result<BlockHash, StorageError> {
        Ok(BlockHash::default())
    }
    async fn add_block_body(
        &self,
        _block_id: BlockHash,
        _block_body: BlockBody,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn get_block_header(&self, _block_id: BlockHash) -> Result<BlockHeader, StorageError> {
        Ok(BlockHeader::default())
    }
}
