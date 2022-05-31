use jsonrpsee::{core::async_trait, types::EmptyParams};

use crate::{
    gateway::{api::JsonRpcServer, Gateway},
    starknet::BlockNumber,
    storage::{StorageError, StorageReader},
};

struct MockStorageReader;

#[async_trait]
impl StorageReader for MockStorageReader {
    async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        Ok(BlockNumber(0))
    }
}

#[tokio::test]
async fn test_block_number() {
    let storage_reader = Box::new(MockStorageReader {});
    let module = Gateway { storage_reader }.into_rpc();
    let result: BlockNumber = module
        .call("starknet_blockNumber", EmptyParams::new())
        .await
        .unwrap();
    assert_eq!(result, BlockNumber(0));
}
