use std::sync::Arc;

use async_trait::async_trait;
use futures_util::pin_mut;
use mockall::{mock, predicate};
use reqwest::StatusCode;
use starknet_api::{BlockNumber, ClassHash, ContractClass};
use starknet_client::{Block, BlockStateUpdate, ClientError, StarknetClientTrait};
use tokio_stream::StreamExt;

use crate::sources::central::GenericCentralSource;
use crate::CentralError;

// Using mock! and not automock because StarknetClient is defined in another crate. For more
// details, See mockall's documentation: https://docs.rs/mockall/latest/mockall/
mock! {
    pub StarknetClient {}

    #[async_trait]
    impl StarknetClientTrait for StarknetClient {
        async fn block_number(&self) -> Result<Option<BlockNumber>, ClientError>;

        async fn block(&self, block_number: BlockNumber) -> Result<Option<Block>, ClientError>;

        async fn class_by_hash(
            &self,
            class_hash: ClassHash
        ) -> Result<Option<ContractClass>, ClientError>;

        async fn state_update(
            &self,
            block_number: BlockNumber,
        ) -> Result<Option<BlockStateUpdate>, ClientError>;
    }
}

#[tokio::test]
async fn last_block_number() {
    let mut mock = MockStarknetClient::new();

    // We need to perform all the mocks before moving the mock into central_source.
    const EXPECTED_LAST_BLOCK_NUMBER: BlockNumber = BlockNumber(9);
    mock.expect_block_number().times(1).returning(|| Ok(Some(EXPECTED_LAST_BLOCK_NUMBER)));

    let central_source = GenericCentralSource { starknet_client: Arc::new(mock) };

    let last_block_number = central_source.get_block_marker().await.unwrap().prev().unwrap();
    assert_eq!(last_block_number, EXPECTED_LAST_BLOCK_NUMBER);
}

#[tokio::test]
async fn stream_block_headers() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 9;
    let mut mock = MockStarknetClient::new();

    // We need to perform all the mocks before moving the mock into central_source.
    for i in START_BLOCK_NUMBER..END_BLOCK_NUMBER {
        mock.expect_block()
            .with(predicate::eq(BlockNumber(i)))
            .times(1)
            .returning(|_block_number| Ok(Some(Block::default())));
    }
    let central_source = GenericCentralSource { starknet_client: Arc::new(mock) };

    let mut expected_block_num = BlockNumber(START_BLOCK_NUMBER);
    let stream =
        central_source.stream_new_blocks(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);
    while let Some(Ok((block_number, _header, _body))) = stream.next().await {
        assert_eq!(expected_block_num, block_number);
        expected_block_num = expected_block_num.next();
    }
}

#[tokio::test]
async fn stream_block_headers_error() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 13;
    const ERROR_BLOCK_NUMBER: u64 = 9;
    let mut mock = MockStarknetClient::new();
    const CODE: StatusCode = StatusCode::NOT_FOUND;
    const MESSAGE: &str = "msg";

    // We need to perform all the mocks before moving the mock into central_source.
    for i in START_BLOCK_NUMBER..ERROR_BLOCK_NUMBER {
        mock.expect_block()
            .with(predicate::eq(BlockNumber(i)))
            .times(1)
            .returning(|_x| Ok(Some(Block::default())));
    }
    mock.expect_block().with(predicate::eq(BlockNumber(ERROR_BLOCK_NUMBER))).times(1).returning(
        |_block_number| {
            Err(ClientError::BadStatusError { code: CODE, message: String::from(MESSAGE) })
        },
    );
    let central_source = GenericCentralSource { starknet_client: Arc::new(mock) };

    let mut expected_block_num = BlockNumber(START_BLOCK_NUMBER);
    let stream =
        central_source.stream_new_blocks(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);
    while let Some(block_tuple) = stream.next().await {
        if expected_block_num == BlockNumber(ERROR_BLOCK_NUMBER) {
            match block_tuple {
                Err(CentralError::ClientError(err_ptr)) => match &*err_ptr {
                    ClientError::BadStatusError { code, message } => {
                        assert_eq!(code, &CODE);
                        assert_eq!(message, MESSAGE);
                    }
                    _ => panic!("Error matching the error: {:?} to the expected error.", &*err_ptr),
                },
                _ => panic!(
                    "Error matching the return value: {:?} to the expected error.",
                    &block_tuple
                ),
            }
        } else {
            let block_number = block_tuple.unwrap().0;
            assert_eq!(expected_block_num, block_number);
        }
        expected_block_num = expected_block_num.next();
    }
    assert_eq!(expected_block_num, BlockNumber(ERROR_BLOCK_NUMBER + 1));
}
