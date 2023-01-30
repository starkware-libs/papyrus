use std::sync::Arc;

use assert_matches::assert_matches;
use futures_util::pin_mut;
use indexmap::IndexMap;
use mockall::predicate;
use reqwest::StatusCode;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StorageKey;
use starknet_api::{patricia_key, stark_felt};
use starknet_client::{
    Block, ClientError, ContractClass, DeployedContract, GlobalRoot, MockStarknetClientTrait,
    StateUpdate, StorageEntry,
};
use tokio_stream::StreamExt;

use crate::sources::central::{CentralError, CentralSourceTrait, GenericCentralSource};

const TEST_CONCURRENT_REQUESTS: usize = 300;

#[tokio::test]
async fn last_block_number() {
    let mut mock = MockStarknetClientTrait::new();

    // We need to perform all the mocks before moving the mock into central_source.
    const EXPECTED_LAST_BLOCK_NUMBER: BlockNumber = BlockNumber(9);
    mock.expect_block_number().times(1).returning(|| Ok(Some(EXPECTED_LAST_BLOCK_NUMBER)));

    let central_source = GenericCentralSource {
        starknet_client: Arc::new(mock),
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
    };

    let last_block_number = central_source.get_block_marker().await.unwrap().prev().unwrap();
    assert_eq!(last_block_number, EXPECTED_LAST_BLOCK_NUMBER);
}

#[tokio::test]
async fn stream_block_headers() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 9;
    let mut mock = MockStarknetClientTrait::new();

    // We need to perform all the mocks before moving the mock into central_source.
    for i in START_BLOCK_NUMBER..END_BLOCK_NUMBER {
        mock.expect_block()
            .with(predicate::eq(BlockNumber(i)))
            .times(1)
            .returning(|_block_number| Ok(Some(Block::default())));
    }
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
    };

    let mut expected_block_num = BlockNumber(START_BLOCK_NUMBER);
    let stream =
        central_source.stream_new_blocks(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);
    while let Some(Ok((block_number, _block))) = stream.next().await {
        assert_eq!(expected_block_num, block_number);
        expected_block_num = expected_block_num.next();
    }
    assert_eq!(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
}

#[tokio::test]
async fn stream_block_headers_some_are_missing() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 13;
    const MISSING_BLOCK_NUMBER: u64 = 9;
    let mut mock = MockStarknetClientTrait::new();

    // We need to perform all the mocks before moving the mock into central_source.
    for i in START_BLOCK_NUMBER..MISSING_BLOCK_NUMBER {
        mock.expect_block()
            .with(predicate::eq(BlockNumber(i)))
            .times(1)
            .returning(|_| Ok(Some(Block::default())));
    }
    mock.expect_block()
        .with(predicate::eq(BlockNumber(MISSING_BLOCK_NUMBER)))
        .times(1)
        .returning(|_| Ok(None));
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
    };

    let mut expected_block_num = BlockNumber(START_BLOCK_NUMBER);
    let stream =
        central_source.stream_new_blocks(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);
    while let Some(block_tuple) = stream.next().await {
        if expected_block_num == BlockNumber(MISSING_BLOCK_NUMBER) {
            assert_matches!(
                block_tuple,
                Err(CentralError::BlockNotFound { block_number })
                if block_number == expected_block_num
            );
        } else {
            let block_number = block_tuple.unwrap().0;
            assert_eq!(expected_block_num, block_number);
        }
        expected_block_num = expected_block_num.next();
    }
    assert_eq!(expected_block_num, BlockNumber(MISSING_BLOCK_NUMBER + 1));
}

#[tokio::test]
async fn stream_block_headers_error() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 13;
    const ERROR_BLOCK_NUMBER: u64 = 9;
    let mut mock = MockStarknetClientTrait::new();
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
            Err(ClientError::BadResponseStatus { code: CODE, message: String::from(MESSAGE) })
        },
    );
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
    };

    let mut expected_block_num = BlockNumber(START_BLOCK_NUMBER);
    let stream =
        central_source.stream_new_blocks(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);
    while let Some(block_tuple) = stream.next().await {
        if expected_block_num == BlockNumber(ERROR_BLOCK_NUMBER) {
            assert_matches!(
                block_tuple,
                Err(CentralError::ClientError(err_ptr))
                if match &*err_ptr {
                    ClientError::BadResponseStatus { code, message } =>
                        code == &CODE && message == MESSAGE,
                    _ => false,
                }
            );
        } else {
            let block_number = block_tuple.unwrap().0;
            assert_eq!(expected_block_num, block_number);
        }
        expected_block_num = expected_block_num.next();
    }
    assert_eq!(expected_block_num, BlockNumber(ERROR_BLOCK_NUMBER + 1));
}

#[tokio::test]
async fn stream_state_updates() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 7;

    let class_hash1 = ClassHash(stark_felt!("0x123"));
    let class_hash2 = ClassHash(stark_felt!("0x456"));
    let class_hash3 = ClassHash(stark_felt!("0x789"));
    let contract_address1 = ContractAddress(patricia_key!("0xabc"));
    let contract_address2 = ContractAddress(patricia_key!("0xdef"));
    let nonce1 = Nonce(stark_felt!("0x123456789abcdef"));
    let root1 = GlobalRoot(stark_felt!("0x111"));
    let root2 = GlobalRoot(stark_felt!("0x222"));
    let block_hash1 = BlockHash(stark_felt!("0x333"));
    let block_hash2 = BlockHash(stark_felt!("0x444"));
    let key = StorageKey(patricia_key!("0x555"));
    let value = stark_felt!("0x666");

    // TODO(shahak): Fill these contract classes with non-empty data.
    let contract_class1 = ContractClass::default();
    let contract_class2 = ContractClass::default();
    let contract_class3 = ContractClass::default();

    let client_state_diff1 = starknet_client::StateDiff {
        storage_diffs: IndexMap::from([(contract_address1, vec![StorageEntry { key, value }])]),
        deployed_contracts: vec![
            DeployedContract { address: contract_address1, class_hash: class_hash2 },
            DeployedContract { address: contract_address2, class_hash: class_hash3 },
        ],
        declared_contracts: vec![class_hash1, class_hash3],
        nonces: IndexMap::from([(contract_address1, nonce1)]),
    };
    let client_state_diff2 = starknet_client::StateDiff::default();

    let block_state_update1 = StateUpdate {
        block_hash: block_hash1,
        new_root: root2,
        old_root: root1,
        state_diff: client_state_diff1,
    };
    let block_state_update2 = StateUpdate {
        block_hash: block_hash2,
        new_root: root2,
        old_root: root2,
        state_diff: client_state_diff2,
    };

    let mut mock = MockStarknetClientTrait::new();
    let block_state_update1_clone = block_state_update1.clone();
    mock.expect_state_update()
        .with(predicate::eq(BlockNumber(START_BLOCK_NUMBER)))
        .times(1)
        .returning(move |_x| Ok(Some(block_state_update1_clone.clone())));
    let block_state_update2_clone = block_state_update2.clone();
    mock.expect_state_update()
        .with(predicate::eq(BlockNumber(START_BLOCK_NUMBER + 1)))
        .times(1)
        .returning(move |_x| Ok(Some(block_state_update2_clone.clone())));
    let contract_class1_clone = contract_class1.clone();
    mock.expect_class_by_hash()
        .with(predicate::eq(class_hash1))
        .times(1)
        .returning(move |_x| Ok(Some(contract_class1_clone.clone())));
    let contract_class2_clone = contract_class2.clone();
    mock.expect_class_by_hash()
        .with(predicate::eq(class_hash2))
        .times(1)
        .returning(move |_x| Ok(Some(contract_class2_clone.clone())));
    let contract_class3_clone = contract_class3.clone();
    mock.expect_class_by_hash()
        .with(predicate::eq(class_hash3))
        .times(1)
        .returning(move |_x| Ok(Some(contract_class3_clone.clone())));

    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
    };
    let initial_block_num = BlockNumber(START_BLOCK_NUMBER);

    let stream =
        central_source.stream_state_updates(initial_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);

    let (current_block_num, current_block_hash, state_diff, deployed_contract_class_definitions) =
        if let Some(Ok(state_diff_tuple)) = stream.next().await {
            state_diff_tuple
        } else {
            panic!("Match of streamed state_update failed!");
        };
    assert_eq!(initial_block_num, current_block_num);
    assert_eq!(block_hash1, current_block_hash);
    assert_eq!(
        IndexMap::from([(class_hash2, starknet_api::state::ContractClass::from(contract_class2))]),
        deployed_contract_class_definitions,
    );

    assert_eq!(
        IndexMap::from([(contract_address1, class_hash2), (contract_address2, class_hash3)]),
        state_diff.deployed_contracts
    );
    assert_eq!(
        IndexMap::from([(contract_address1, IndexMap::from([(key, value)]))]),
        state_diff.storage_diffs
    );
    assert_eq!(
        IndexMap::from([
            (class_hash1, starknet_api::state::ContractClass::from(contract_class1)),
            (class_hash3, starknet_api::state::ContractClass::from(contract_class3)),
        ]),
        state_diff.declared_classes,
    );
    assert_eq!(IndexMap::from([(contract_address1, nonce1)]), state_diff.nonces);

    let (current_block_num, current_block_hash, state_diff, _deployed_classes) =
        if let Some(Ok(state_diff_tuple)) = stream.next().await {
            state_diff_tuple
        } else {
            panic!("Match of streamed state_update failed!");
        };
    assert_eq!(initial_block_num.next(), current_block_num);
    assert_eq!(block_hash2, current_block_hash);
    assert_eq!(state_diff, starknet_api::state::StateDiff::default());

    assert!(stream.next().await.is_none());
}
