use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use futures_util::StreamExt;
use indexmap::IndexMap;
use papyrus_common::pending_classes::{ApiContractClass, PendingClasses, PendingClassesTrait};
use papyrus_storage::base_layer::BaseLayerStorageReader;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageReader, StorageWriter};
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::GENESIS_HASH;
use starknet_api::patricia_key;
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingStateUpdate};
use starknet_client::reader::objects::state::StateDiff as ClientStateDiff;
use starknet_client::reader::objects::transaction::Transaction as ClientTransaction;
use starknet_client::reader::{DeclaredClassHashEntry, PendingData};
use starknet_types_core::felt::Felt;
use test_utils::{get_rng, GetTestInstance};
use tokio::sync::RwLock;

use crate::sources::base_layer::MockBaseLayerSourceTrait;
use crate::sources::central::MockCentralSourceTrait;
use crate::sources::pending::MockPendingSourceTrait;
use crate::{
    sort_state_diff,
    stream_new_base_layer_block,
    sync_pending_data,
    GenericStateSync,
    StateSyncError,
    SyncConfig,
    SyncEvent,
};

// TODO(anatg): Add a test to check that the sync calls the sort_state_diff function
// before writing to the storage.
#[test]
fn state_sorted() {
    let hash0 = Felt::ZERO;
    let patricia_key0 = patricia_key!(0x0);
    let hash1 = Felt::ONE;
    let patricia_key1 = patricia_key!(0x1);

    let dep_contract_0 = (ContractAddress(patricia_key0), ClassHash(hash0));
    let dep_contract_1 = (ContractAddress(patricia_key1), ClassHash(hash1));
    let storage_key_0 = StorageKey(patricia_key!(0x0));
    let storage_key_1 = StorageKey(patricia_key!(0x1));
    let declare_class_0 =
        (ClassHash(hash0), (CompiledClassHash::default(), ContractClass::default()));
    let declare_class_1 =
        (ClassHash(hash1), (CompiledClassHash::default(), ContractClass::default()));
    let deprecated_declared_0 = (ClassHash(hash0), DeprecatedContractClass::default());
    let deprecated_declared_1 = (ClassHash(hash1), DeprecatedContractClass::default());
    let nonce_0 = (ContractAddress(patricia_key0), Nonce(hash0));
    let nonce_1 = (ContractAddress(patricia_key1), Nonce(hash1));
    let replaced_class_0 = (ContractAddress(patricia_key0), ClassHash(hash0));
    let replaced_class_1 = (ContractAddress(patricia_key1), ClassHash(hash1));

    let unsorted_deployed_contracts = IndexMap::from([dep_contract_1, dep_contract_0]);
    let unsorted_declared_classes =
        IndexMap::from([declare_class_1.clone(), declare_class_0.clone()]);
    let unsorted_deprecated_declared =
        IndexMap::from([deprecated_declared_1.clone(), deprecated_declared_0.clone()]);
    let unsorted_nonces = IndexMap::from([nonce_1, nonce_0]);
    let unsorted_storage_entries = IndexMap::from([(storage_key_1, hash1), (storage_key_0, hash0)]);
    let unsorted_storage_diffs = IndexMap::from([
        (ContractAddress(patricia_key1), unsorted_storage_entries.clone()),
        (ContractAddress(patricia_key0), unsorted_storage_entries),
    ]);
    let unsorted_replaced_classes = IndexMap::from([replaced_class_1, replaced_class_0]);

    let mut state_diff = StateDiff {
        deployed_contracts: unsorted_deployed_contracts,
        storage_diffs: unsorted_storage_diffs,
        deprecated_declared_classes: unsorted_deprecated_declared,
        declared_classes: unsorted_declared_classes,
        nonces: unsorted_nonces,
        replaced_classes: unsorted_replaced_classes,
    };

    let sorted_deployed_contracts = IndexMap::from([dep_contract_0, dep_contract_1]);
    let sorted_declared_classes = IndexMap::from([declare_class_0, declare_class_1]);
    let sorted_deprecated_declared = IndexMap::from([deprecated_declared_0, deprecated_declared_1]);
    let sorted_nonces = IndexMap::from([nonce_0, nonce_1]);
    let sorted_storage_entries = IndexMap::from([(storage_key_0, hash0), (storage_key_1, hash1)]);
    let sorted_storage_diffs = IndexMap::from([
        (ContractAddress(patricia_key0), sorted_storage_entries.clone()),
        (ContractAddress(patricia_key1), sorted_storage_entries.clone()),
    ]);
    let sorted_replaced_classes = IndexMap::from([replaced_class_0, replaced_class_1]);

    sort_state_diff(&mut state_diff);
    assert_eq!(
        state_diff.deployed_contracts.get_index(0).unwrap(),
        sorted_deployed_contracts.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.declared_classes.get_index(0).unwrap(),
        sorted_declared_classes.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.deprecated_declared_classes.get_index(0).unwrap(),
        sorted_deprecated_declared.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.storage_diffs.get_index(0).unwrap(),
        sorted_storage_diffs.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.storage_diffs.get_index(0).unwrap().1.get_index(0).unwrap(),
        sorted_storage_entries.get_index(0).unwrap(),
    );
    assert_eq!(state_diff.nonces.get_index(0).unwrap(), sorted_nonces.get_index(0).unwrap());
    assert_eq!(
        state_diff.replaced_classes.get_index(0).unwrap(),
        sorted_replaced_classes.get_index(0).unwrap(),
    );
}

#[tokio::test]
async fn stream_new_base_layer_block_test_header_marker() {
    let (reader, mut writer) = get_test_storage().0;

    // Header marker points to to block number 5.
    add_headers(5, &mut writer);

    // TODO(dvir): find a better way to do it.
    // Base layer after the header marker, skip 5 and 10 and return only 1 and 4.
    let block_numbers = vec![5, 1, 10, 4];
    let mut iter = block_numbers.into_iter().map(|bn| (BlockNumber(bn), BlockHash::default()));
    let mut mock = MockBaseLayerSourceTrait::new();
    mock.expect_latest_proved_block().times(4).returning(move || Ok(iter.next()));
    let mut stream =
        stream_new_base_layer_block(reader, Arc::new(mock), Duration::from_millis(0)).boxed();

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(1), .. });

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(4), .. });
}

#[tokio::test]
async fn stream_new_base_layer_block_no_blocks_on_base_layer() {
    let (reader, mut writer) = get_test_storage().0;

    // Header marker points to to block number 5.
    add_headers(5, &mut writer);

    // In the first polling of the base layer no blocks were found, in the second polling a block
    // was found.
    let mut values = vec![None, Some((BlockNumber(1), BlockHash::default()))].into_iter();
    let mut mock = MockBaseLayerSourceTrait::new();
    mock.expect_latest_proved_block().times(2).returning(move || Ok(values.next().unwrap()));

    let mut stream =
        stream_new_base_layer_block(reader, Arc::new(mock), Duration::from_millis(0)).boxed();

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(1), .. });
}

#[test]
fn store_base_layer_block_test() {
    let (reader, mut writer) = get_test_storage().0;

    let header_hash = BlockHash(Felt::ZERO);
    let header = BlockHeader {
        block_number: BlockNumber(0),
        block_hash: header_hash,
        ..BlockHeader::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &header)
        .unwrap()
        .commit()
        .unwrap();

    let mut gen_state_sync = GenericStateSync {
        config: SyncConfig::default(),
        shared_highest_block: Arc::new(RwLock::new(None)),
        pending_data: Arc::new(RwLock::new(PendingData::default())),
        central_source: Arc::new(MockCentralSourceTrait::new()),
        pending_source: Arc::new(MockPendingSourceTrait::new()),
        pending_classes: Arc::new(RwLock::new(PendingClasses::default())),
        base_layer_source: Arc::new(MockBaseLayerSourceTrait::new()),
        reader,
        writer,
    };

    // Trying to store a block without a header in the storage.
    let res = gen_state_sync.store_base_layer_block(BlockNumber(1), BlockHash::default());
    assert_matches!(res, Err(StateSyncError::BaseLayerBlockWithoutMatchingHeader { .. }));

    // Trying to store a block with mismatching header.
    let res = gen_state_sync
        .store_base_layer_block(BlockNumber(0), BlockHash(Felt::from_hex_unchecked("0x666")));
    assert_matches!(res, Err(StateSyncError::BaseLayerHashMismatch { .. }));

    // Happy flow.
    let res = gen_state_sync.store_base_layer_block(BlockNumber(0), header_hash);
    assert!(res.is_ok());
    let base_layer_marker =
        gen_state_sync.reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
    assert_eq!(base_layer_marker, BlockNumber(1));
}

// Adds to the storage 'headers_num' headers.
fn add_headers(headers_num: u64, writer: &mut StorageWriter) {
    for i in 0..headers_num {
        let header = BlockHeader {
            block_number: BlockNumber(i),
            block_hash: BlockHash(i.into()),
            ..BlockHeader::default()
        };
        writer
            .begin_rw_txn()
            .unwrap()
            .append_header(BlockNumber(i), &header)
            .unwrap()
            .commit()
            .unwrap();
    }
}

#[allow(clippy::too_many_arguments)]
async fn test_pending_sync(
    reader: StorageReader,
    old_pending_data: PendingData,
    new_pending_datas: Vec<PendingData>,
    expected_pending_data: PendingData,
    old_pending_classes_data: Option<PendingClasses>,
    // Verifies that the classes will be requested in the given order.
    new_pending_classes: Vec<(ClassHash, ApiContractClass)>,
    new_pending_compiled_classes: Vec<(ClassHash, CasmContractClass)>,
    expected_pending_classes: Option<PendingClasses>,
) {
    let mut mock_pending_source = MockPendingSourceTrait::new();
    let mut mock_central_source = MockCentralSourceTrait::new();
    let pending_data_lock = Arc::new(RwLock::new(old_pending_data));
    let pending_classes_lock = Arc::new(RwLock::new(old_pending_classes_data.unwrap_or_default()));

    for new_pending_data in new_pending_datas {
        mock_pending_source
            .expect_get_pending_data()
            .times(1)
            .return_once(move || Ok(new_pending_data));
    }

    for (expected_class_hash, new_pending_class) in new_pending_classes {
        mock_central_source.expect_get_class().times(1).return_once(move |class_hash| {
            assert_eq!(class_hash, expected_class_hash);
            Ok(new_pending_class)
        });
    }
    for (expected_class_hash, new_pending_compiled_class) in new_pending_compiled_classes {
        mock_central_source.expect_get_compiled_class().times(1).return_once(move |class_hash| {
            assert_eq!(class_hash, expected_class_hash);
            Ok(new_pending_compiled_class)
        });
    }

    sync_pending_data(
        reader,
        Arc::new(mock_central_source),
        Arc::new(mock_pending_source),
        pending_data_lock.clone(),
        pending_classes_lock.clone(),
        Duration::ZERO,
    )
    .await
    .unwrap();

    assert_eq!(pending_data_lock.read().await.clone(), expected_pending_data);
    if let Some(expected_pending_classes) = expected_pending_classes {
        assert_eq!(pending_classes_lock.read().await.clone(), expected_pending_classes);
    }
}

#[tokio::test]
async fn pending_sync_advances_only_when_new_data_has_more_transactions() {
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with no block headers.
    let (reader, _) = get_test_storage().0;
    let mut rng = get_rng();

    let old_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: genesis_hash,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        ..Default::default()
    };
    let advanced_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: genesis_hash,
            transactions: vec![
                ClientTransaction::get_test_instance(&mut rng),
                ClientTransaction::get_test_instance(&mut rng),
                ClientTransaction::get_test_instance(&mut rng),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let less_advanced_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: genesis_hash,
            transactions: vec![
                ClientTransaction::get_test_instance(&mut rng),
                ClientTransaction::get_test_instance(&mut rng),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_block_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: BlockHash(Felt::ONE), ..Default::default() },
        ..Default::default()
    };

    let new_pending_datas =
        vec![advanced_pending_data.clone(), less_advanced_pending_data, new_block_pending_data];
    let expected_pending_data = advanced_pending_data;
    let old_pending_classes_data = None;
    let new_pending_classes = vec![];
    let new_pending_compiled_classes = vec![];
    let expected_pending_classes = None;
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        old_pending_classes_data,
        new_pending_classes,
        new_pending_compiled_classes,
        expected_pending_classes,
    )
    .await
}

#[tokio::test]
async fn pending_sync_new_data_has_more_advanced_hash_and_less_transactions() {
    const FIRST_BLOCK_HASH: BlockHash = BlockHash(Felt::ONE);
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with one block header.
    let (reader, mut writer) = get_test_storage().0;
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                block_hash: FIRST_BLOCK_HASH,
                parent_hash: genesis_hash,
                block_number: BlockNumber(0),
                ..Default::default()
            },
        )
        .unwrap()
        .commit()
        .unwrap();
    let mut rng = get_rng();

    let old_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: genesis_hash,
            transactions: vec![
                ClientTransaction::get_test_instance(&mut rng),
                ClientTransaction::get_test_instance(&mut rng),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: FIRST_BLOCK_HASH,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_block_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: BlockHash(Felt::TWO), ..Default::default() },
        ..Default::default()
    };

    let new_pending_datas = vec![new_pending_data.clone(), new_block_pending_data];
    let expected_pending_data = new_pending_data;
    let old_pending_classes_data = None;
    let new_pending_classes = vec![];
    let new_pending_compiled_classes = vec![];
    let expected_pending_classes = None;
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        old_pending_classes_data,
        new_pending_classes,
        new_pending_compiled_classes,
        expected_pending_classes,
    )
    .await
}

#[tokio::test]
async fn pending_sync_stops_when_data_has_block_hash_field_with_a_different_hash() {
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with no block headers.
    let (reader, _) = get_test_storage().0;
    let mut rng = get_rng();

    let old_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: genesis_hash,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_pending_datas = vec![PendingData {
        block: PendingBlock {
            block_hash: Some(BlockHash(Felt::ONE)),
            parent_block_hash: genesis_hash,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        ..Default::default()
    }];
    let expected_pending_data = old_pending_data.clone();
    let old_pending_classes_data = None;
    let new_pending_classes = vec![];
    let new_pending_compiled_classes = vec![];
    let expected_pending_classes = None;
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        old_pending_classes_data,
        new_pending_classes,
        new_pending_compiled_classes,
        expected_pending_classes,
    )
    .await
}

#[tokio::test]
async fn pending_sync_doesnt_stop_when_data_has_block_hash_field_with_the_same_hash() {
    const FIRST_BLOCK_HASH: BlockHash = BlockHash(Felt::ONE);
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with one block header.
    let (reader, mut writer) = get_test_storage().0;
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                block_hash: FIRST_BLOCK_HASH,
                parent_hash: genesis_hash,
                block_number: BlockNumber(0),
                ..Default::default()
            },
        )
        .unwrap()
        .commit()
        .unwrap();
    let mut rng = get_rng();

    let old_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: FIRST_BLOCK_HASH,
            transactions: vec![
                ClientTransaction::get_test_instance(&mut rng),
                ClientTransaction::get_test_instance(&mut rng),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_pending_data = PendingData {
        block: PendingBlock {
            block_hash: Some(FIRST_BLOCK_HASH),
            parent_block_hash: genesis_hash,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_block_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: BlockHash(Felt::TWO), ..Default::default() },
        ..Default::default()
    };

    let new_pending_datas = vec![new_pending_data, new_block_pending_data];
    let expected_pending_data = old_pending_data.clone();
    let old_pending_classes_data = None;
    let new_pending_classes = vec![];
    let new_pending_compiled_classes = vec![];
    let expected_pending_classes = None;
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        old_pending_classes_data,
        new_pending_classes,
        new_pending_compiled_classes,
        expected_pending_classes,
    )
    .await
}

#[tokio::test]
async fn pending_sync_updates_when_data_has_block_hash_field_with_the_same_hash_and_more_transactions()
 {
    const FIRST_BLOCK_HASH: BlockHash = BlockHash(Felt::ONE);
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with one block header.
    let (reader, mut writer) = get_test_storage().0;
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                block_hash: FIRST_BLOCK_HASH,
                parent_hash: genesis_hash,
                block_number: BlockNumber(0),
                ..Default::default()
            },
        )
        .unwrap()
        .commit()
        .unwrap();
    let mut rng = get_rng();

    let old_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: FIRST_BLOCK_HASH,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_pending_data = PendingData {
        block: PendingBlock {
            block_hash: Some(FIRST_BLOCK_HASH),
            parent_block_hash: genesis_hash,
            transactions: vec![
                ClientTransaction::get_test_instance(&mut rng),
                ClientTransaction::get_test_instance(&mut rng),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let new_block_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: BlockHash(Felt::TWO), ..Default::default() },
        ..Default::default()
    };

    let new_pending_datas = vec![new_pending_data.clone(), new_block_pending_data];
    let expected_pending_data = new_pending_data;
    let old_pending_classes_data = None;
    let new_pending_classes = vec![];
    let new_pending_compiled_classes = vec![];
    let expected_pending_classes = None;
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        old_pending_classes_data,
        new_pending_classes,
        new_pending_compiled_classes,
        expected_pending_classes,
    )
    .await
}

#[tokio::test]
async fn pending_sync_classes_request_only_new_classes() {
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with no blocks.
    let (reader, _writer) = get_test_storage().0;
    let mut rng = get_rng();

    let first_class_hash = ClassHash(Felt::ONE);
    let second_class_hash = ClassHash(Felt::TWO);

    let first_new_pending_data = PendingData {
        block: PendingBlock {
            parent_block_hash: genesis_hash,
            transactions: vec![ClientTransaction::get_test_instance(&mut rng)],
            ..Default::default()
        },
        state_update: PendingStateUpdate {
            state_diff: ClientStateDiff {
                declared_classes: vec![DeclaredClassHashEntry {
                    class_hash: first_class_hash,
                    compiled_class_hash: CompiledClassHash(Felt::ONE),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
    };
    let mut second_new_pending_data = first_new_pending_data.clone();
    second_new_pending_data.block.transactions.push(ClientTransaction::get_test_instance(&mut rng));
    second_new_pending_data.state_update.state_diff.old_declared_contracts.push(second_class_hash);
    let new_block_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: BlockHash(Felt::ONE), ..Default::default() },
        ..Default::default()
    };

    let first_class = ApiContractClass::DeprecatedContractClass(
        DeprecatedContractClass::get_test_instance(&mut rng),
    );
    let second_class = ApiContractClass::ContractClass(ContractClass::get_test_instance(&mut rng));
    let compiled_class = CasmContractClass::get_test_instance(&mut rng);

    let mut expected_pending_classes = PendingClasses::default();
    expected_pending_classes.add_class(first_class_hash, first_class.clone());
    expected_pending_classes.add_class(second_class_hash, second_class.clone());
    expected_pending_classes.add_compiled_class(first_class_hash, compiled_class.clone());

    let old_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: genesis_hash, ..Default::default() },
        ..Default::default()
    };
    let new_pending_datas =
        vec![first_new_pending_data, second_new_pending_data.clone(), new_block_pending_data];
    let expected_pending_data = second_new_pending_data;
    let old_pending_classes_data = PendingClasses::default();
    let new_pending_classes =
        vec![(first_class_hash, first_class.clone()), (second_class_hash, second_class.clone())];
    let new_pending_compiled_classes = vec![(first_class_hash, compiled_class.clone())];
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        Some(old_pending_classes_data),
        new_pending_classes,
        new_pending_compiled_classes,
        Some(expected_pending_classes),
    )
    .await
}

#[tokio::test]
async fn pending_sync_classes_are_cleaned_on_first_pending_data_from_latest_block() {
    const FIRST_BLOCK_HASH: BlockHash = BlockHash(Felt::ONE);
    let genesis_hash = BlockHash(Felt::from(GENESIS_HASH));
    // Storage with one block header.
    let (reader, mut writer) = get_test_storage().0;
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                block_hash: FIRST_BLOCK_HASH,
                parent_hash: genesis_hash,
                block_number: BlockNumber(0),
                ..Default::default()
            },
        )
        .unwrap()
        .commit()
        .unwrap();
    let mut rng = get_rng();

    let old_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: genesis_hash, ..Default::default() },
        ..Default::default()
    };
    let new_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: FIRST_BLOCK_HASH, ..Default::default() },
        ..Default::default()
    };
    let new_block_pending_data = PendingData {
        block: PendingBlock { parent_block_hash: BlockHash(Felt::TWO), ..Default::default() },
        ..Default::default()
    };

    let mut old_pending_classes_data = PendingClasses::default();
    old_pending_classes_data.add_class(
        ClassHash(Felt::ONE),
        ApiContractClass::DeprecatedContractClass(DeprecatedContractClass::get_test_instance(
            &mut rng,
        )),
    );
    old_pending_classes_data
        .add_compiled_class(ClassHash(Felt::TWO), CasmContractClass::get_test_instance(&mut rng));

    let new_pending_datas = vec![new_pending_data.clone(), new_block_pending_data];
    let expected_pending_data = new_pending_data;
    let new_pending_classes = vec![];
    let new_pending_compiled_classes = vec![];
    let expected_pending_classes = PendingClasses::default();
    test_pending_sync(
        reader,
        old_pending_data,
        new_pending_datas,
        expected_pending_data,
        Some(old_pending_classes_data),
        new_pending_classes,
        new_pending_compiled_classes,
        Some(expected_pending_classes),
    )
    .await
}
