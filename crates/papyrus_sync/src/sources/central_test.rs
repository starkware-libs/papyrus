use async_trait::async_trait;
use futures_util::pin_mut;
use mockall::{mock, predicate};
use starknet_api::{BlockNumber, ClassHash, ContractClass};
use starknet_client::{Block, BlockStateUpdate, ClientError, StarknetClientTrait};
use tokio_stream::StreamExt;

use crate::sources::central::GenericCentralSource;

// Using mock! and not automock because StarknetClient is defined in another crate. For more
// details, See mockall's documentation.
mock! {
    pub StarknetClient {}

    #[async_trait]
    impl StarknetClientTrait for StarknetClient {
        async fn block_number(&self) -> Result<Option<BlockNumber>, ClientError>;

        async fn block(&self, block_number: BlockNumber) -> Result<Option<Block>, ClientError>;

        async fn class_by_hash(&self, class_hash: ClassHash) -> Result<ContractClass, ClientError>;

        async fn state_update(
            &self,
            block_number: BlockNumber,
        ) -> Result<BlockStateUpdate, ClientError>;
    }
}

#[tokio::test]
async fn last_block_number() {
    let mut mock = MockStarknetClient::new();

    // We need to perform all the mocks before moving the mock into central_source.
    const EXPECTED_LAST_BLOCK_NUMBER: BlockNumber = BlockNumber(9);
    mock.expect_block_number().times(1).returning(|| Ok(Some(EXPECTED_LAST_BLOCK_NUMBER)));

    let central_source = GenericCentralSource { starknet_client: mock };

    let last_block_number = central_source.get_block_marker().await.unwrap().prev().unwrap();
    assert_eq!(last_block_number, EXPECTED_LAST_BLOCK_NUMBER);
}

#[tokio::test]
async fn stream_block_headers() {
    const START: u64 = 5;
    const END: u64 = 9;
    let mut mock = MockStarknetClient::new();

    // We need to perform all the mocks before moving the mock into central_source.
    for i in START..END {
        mock.expect_block()
            .with(predicate::eq(BlockNumber(i)))
            .times(1)
            .returning(|_x| Ok(Some(Block::default())));
    }
    let central_source = GenericCentralSource { starknet_client: mock };

    let mut expected_block_num = BlockNumber(START);
    let stream = central_source.stream_new_blocks(expected_block_num, BlockNumber(END));
    pin_mut!(stream);
    while let Some(Ok((block_number, _header, _body))) = stream.next().await {
        assert_eq!(expected_block_num, block_number);
        expected_block_num = expected_block_num.next();
    }
}
