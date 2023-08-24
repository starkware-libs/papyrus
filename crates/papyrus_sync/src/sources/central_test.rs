use std::sync::Arc;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use futures_util::pin_mut;
use indexmap::{indexmap, IndexMap};
use mockall::predicate;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use pretty_assertions::assert_eq;
use reqwest::StatusCode;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass as sn_api_ContractClass, StateDiff, StorageKey};
use starknet_api::{patricia_key, stark_felt};
use starknet_client::reader::{
    Block,
    ContractClass,
    DeclaredClassHashEntry,
    DeployedContract,
    GenericContractClass,
    GlobalRoot,
    MockStarknetReader,
    ReaderClientError,
    ReplacedClass,
    StateUpdate,
    StorageEntry,
};
use starknet_client::ClientError;
use tokio_stream::StreamExt;

use super::state_update_stream::StateUpdateStreamConfig;
use crate::sources::central::{CentralError, CentralSourceTrait, GenericCentralSource};

const TEST_CONCURRENT_REQUESTS: usize = 300;

#[tokio::test]
async fn last_block_number() {
    let mut mock = MockStarknetReader::new();

    // We need to perform all the mocks before moving the mock into central_source.
    const EXPECTED_LAST_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
    mock.expect_latest_block().times(1).returning(|| {
        Ok(Some(Block { block_number: EXPECTED_LAST_BLOCK_NUMBER, ..Default::default() }))
    });

    let ((reader, _), _temp_dir) = get_test_storage();
    let central_source = GenericCentralSource {
        starknet_client: Arc::new(mock),
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        storage_reader: reader,
        state_update_stream_config: state_update_stream_config_for_test(),
    };

    let last_block_number = central_source.get_latest_block().await.unwrap().unwrap().block_number;
    assert_eq!(last_block_number, EXPECTED_LAST_BLOCK_NUMBER);
}

#[tokio::test]
async fn stream_block_headers() {
    const START_BLOCK_NUMBER: u64 = 5;
    const END_BLOCK_NUMBER: u64 = 9;
    let mut mock = MockStarknetReader::new();

    // We need to perform all the mocks before moving the mock into central_source.
    for i in START_BLOCK_NUMBER..END_BLOCK_NUMBER {
        mock.expect_block()
            .with(predicate::eq(BlockNumber(i)))
            .times(1)
            .returning(|_block_number| Ok(Some(Block::default())));
    }
    let ((reader, _), _temp_dir) = get_test_storage();
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
        storage_reader: reader,
        state_update_stream_config: state_update_stream_config_for_test(),
    };

    let mut expected_block_num = BlockNumber(START_BLOCK_NUMBER);
    let stream =
        central_source.stream_new_blocks(expected_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);
    while let Some(Ok((block_number, _block, _starknet_version))) = stream.next().await {
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
    let mut mock = MockStarknetReader::new();

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
    let ((reader, _), _temp_dir) = get_test_storage();
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
        storage_reader: reader,
        state_update_stream_config: state_update_stream_config_for_test(),
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
    let mut mock = MockStarknetReader::new();
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
            Err(ReaderClientError::ClientError(ClientError::BadResponseStatus {
                code: CODE,
                message: String::from(MESSAGE),
            }))
        },
    );
    let ((reader, _), _temp_dir) = get_test_storage();
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
        storage_reader: reader,
        state_update_stream_config: state_update_stream_config_for_test(),
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
                    ReaderClientError::ClientError(ClientError::BadResponseStatus { code, message }) =>
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
    let class_hash4 = ClassHash(stark_felt!("0x101112"));
    let contract_address1 = ContractAddress(patricia_key!("0xabc"));
    let contract_address2 = ContractAddress(patricia_key!("0xdef"));
    let contract_address3 = ContractAddress(patricia_key!("0x0abc"));
    let nonce1 = Nonce(stark_felt!("0x123456789abcdef"));
    let root1 = GlobalRoot(stark_felt!("0x111"));
    let root2 = GlobalRoot(stark_felt!("0x222"));
    let block_hash1 = BlockHash(stark_felt!("0x333"));
    let block_hash2 = BlockHash(stark_felt!("0x444"));
    let key = StorageKey(patricia_key!("0x555"));
    let value = stark_felt!("0x666");

    // TODO(shahak): Fill these contract classes with non-empty data.
    let deprecated_contract_class1 = DeprecatedContractClass::default();
    let deprecated_contract_class2 = DeprecatedContractClass::default();
    let deprecated_contract_class3 = DeprecatedContractClass::default();

    let contract_class1 = ContractClass::default();
    let contract_class2 = ContractClass::default();
    let new_class_hash1 = ClassHash(stark_felt!("0x111"));
    let new_class_hash2 = ClassHash(stark_felt!("0x222"));
    let compiled_class_hash1 = CompiledClassHash(stark_felt!("0x00111"));
    let compiled_class_hash2 = CompiledClassHash(stark_felt!("0x00222"));
    let class_hash_entry1 = DeclaredClassHashEntry {
        class_hash: new_class_hash1,
        compiled_class_hash: compiled_class_hash1,
    };
    let class_hash_entry2 = DeclaredClassHashEntry {
        class_hash: new_class_hash2,
        compiled_class_hash: compiled_class_hash2,
    };

    let client_state_diff1 = starknet_client::reader::StateDiff {
        storage_diffs: IndexMap::from([(contract_address1, vec![StorageEntry { key, value }])]),
        deployed_contracts: vec![
            DeployedContract { address: contract_address1, class_hash: class_hash2 },
            DeployedContract { address: contract_address2, class_hash: class_hash3 },
        ],
        old_declared_contracts: vec![class_hash1, class_hash3],
        declared_classes: vec![class_hash_entry1, class_hash_entry2],
        nonces: IndexMap::from([(contract_address1, nonce1)]),
        replaced_classes: vec![ReplacedClass {
            address: contract_address3,
            class_hash: class_hash4,
        }],
    };
    let client_state_diff2 = starknet_client::reader::StateDiff::default();

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

    let mut mock = MockStarknetReader::new();
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
    let new_contract_class1_clone = contract_class1.clone();
    mock.expect_class_by_hash().with(predicate::eq(new_class_hash1)).times(1).returning(
        move |_x| {
            Ok(Some(GenericContractClass::Cairo1ContractClass(new_contract_class1_clone.clone())))
        },
    );
    let new_contract_class2_clone = contract_class2.clone();
    mock.expect_class_by_hash().with(predicate::eq(new_class_hash2)).times(1).returning(
        move |_x| {
            Ok(Some(GenericContractClass::Cairo1ContractClass(new_contract_class2_clone.clone())))
        },
    );
    let contract_class1_clone = deprecated_contract_class1.clone();
    mock.expect_class_by_hash().with(predicate::eq(class_hash1)).times(1).returning(move |_x| {
        Ok(Some(GenericContractClass::Cairo0ContractClass(contract_class1_clone.clone())))
    });
    let contract_class2_clone = deprecated_contract_class2.clone();
    mock.expect_class_by_hash().with(predicate::eq(class_hash2)).times(1).returning(move |_x| {
        Ok(Some(GenericContractClass::Cairo0ContractClass(contract_class2_clone.clone())))
    });
    let contract_class3_clone = deprecated_contract_class3.clone();
    mock.expect_class_by_hash().with(predicate::eq(class_hash3)).times(1).returning(move |_x| {
        Ok(Some(GenericContractClass::Cairo0ContractClass(contract_class3_clone.clone())))
    });
    let ((reader, _), _temp_dir) = get_test_storage();
    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
        storage_reader: reader,
        state_update_stream_config: state_update_stream_config_for_test(),
    };
    let initial_block_num = BlockNumber(START_BLOCK_NUMBER);

    let stream =
        central_source.stream_state_updates(initial_block_num, BlockNumber(END_BLOCK_NUMBER));
    pin_mut!(stream);

    let Some(Ok(state_diff_tuple)) = stream.next().await else {
        panic!("Match of streamed state_update failed!");
    };
    let (current_block_num, current_block_hash, state_diff, deployed_contract_class_definitions) =
        state_diff_tuple;

    assert_eq!(initial_block_num, current_block_num);
    assert_eq!(block_hash1, current_block_hash);
    assert_eq!(
        IndexMap::from([(class_hash2, deprecated_contract_class2)]),
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
            (class_hash1, deprecated_contract_class1),
            (class_hash3, deprecated_contract_class3),
        ]),
        state_diff.deprecated_declared_classes,
    );
    assert_eq!(
        IndexMap::from([
            (
                new_class_hash1,
                (compiled_class_hash1, starknet_api::state::ContractClass::from(contract_class1))
            ),
            (
                new_class_hash2,
                (compiled_class_hash2, starknet_api::state::ContractClass::from(contract_class2))
            ),
        ]),
        state_diff.declared_classes,
    );
    assert_eq!(IndexMap::from([(contract_address1, nonce1)]), state_diff.nonces);
    assert_eq!(IndexMap::from([(contract_address3, class_hash4)]), state_diff.replaced_classes);

    let Some(Ok(state_diff_tuple)) = stream.next().await else {
        panic!("Match of streamed state_update failed!");
    };
    let (current_block_num, current_block_hash, state_diff, _deployed_classes) = state_diff_tuple;

    assert_eq!(initial_block_num.next(), current_block_num);
    assert_eq!(block_hash2, current_block_hash);
    assert_eq!(state_diff, starknet_api::state::StateDiff::default());

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn stream_compiled_classes() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    writer.begin_rw_txn().unwrap().append_state_diff(
        BlockNumber(0),
        StateDiff {
            deployed_contracts: indexmap! {},
            storage_diffs: indexmap! {},
            declared_classes: indexmap! {
                ClassHash(stark_felt!("0x0")) => (CompiledClassHash(stark_felt!("0x0")), sn_api_ContractClass::default()),
                ClassHash(stark_felt!("0x1")) => (CompiledClassHash(stark_felt!("0x1")), sn_api_ContractClass::default())
            },
            deprecated_declared_classes: indexmap! {},
            nonces: indexmap! {},
            replaced_classes: indexmap! {},
        },
        indexmap! {},
    ).unwrap().append_state_diff(
        BlockNumber(1),
        StateDiff {
            deployed_contracts: indexmap! {},
            storage_diffs: indexmap! {},
            declared_classes: indexmap! {
                ClassHash(stark_felt!("0x2")) => (CompiledClassHash(stark_felt!("0x2")), sn_api_ContractClass::default()),
                ClassHash(stark_felt!("0x3")) => (CompiledClassHash(stark_felt!("0x3")), sn_api_ContractClass::default())
            },
            deprecated_declared_classes: indexmap! {},
            nonces: indexmap! {},
            replaced_classes: indexmap! {},
        },
        indexmap! {},
    ).unwrap().commit().unwrap();

    let felts: Vec<_> = (0..4).map(|i| stark_felt!(format!("0x{i}").as_str())).collect();
    let mut mock = MockStarknetReader::new();
    for felt in felts.clone() {
        mock.expect_compiled_class_by_hash()
            .with(predicate::eq(ClassHash(felt)))
            .times(1)
            .returning(move |_x| Ok(Some(CasmContractClass::default())));
    }

    let central_source = GenericCentralSource {
        concurrent_requests: TEST_CONCURRENT_REQUESTS,
        starknet_client: Arc::new(mock),
        storage_reader: reader,
        state_update_stream_config: state_update_stream_config_for_test(),
    };

    let stream = central_source.stream_compiled_classes(BlockNumber(0), BlockNumber(2));
    pin_mut!(stream);

    let expected_compiled_class = CasmContractClass::default();
    for felt in felts {
        let (class_hash, compiled_class_hash, compiled_class) =
            stream.next().await.unwrap().unwrap();
        let expected_class_hash = ClassHash(felt);
        let expected_compiled_class_hash = CompiledClassHash(felt);
        assert_eq!(class_hash, expected_class_hash);
        assert_eq!(compiled_class_hash, expected_compiled_class_hash);
        assert_eq!(compiled_class, expected_compiled_class);
    }
}

fn state_update_stream_config_for_test() -> StateUpdateStreamConfig {
    StateUpdateStreamConfig {
        max_state_updates_to_download: 10,
        max_state_updates_to_store_in_memory: 10,
        max_classes_to_download: 10,
    }
}
