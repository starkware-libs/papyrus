use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use futures_util::StreamExt;
use indexmap::IndexMap;
use mockall::predicate;
use papyrus_common::pending_classes::{PendingClass, PendingClasses};
use papyrus_storage::base_layer::BaseLayerStorageReader;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, GasPrice};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_api::{patricia_key, stark_felt};
use starknet_client::reader::{
    DeclaredClassHashEntry,
    GenericContractClass,
    MockStarknetReader,
    PendingData,
};
use tokio::sync::RwLock;

use crate::sources::base_layer::MockBaseLayerSourceTrait;
use crate::sources::central::MockCentralSourceTrait;
use crate::sources::pending::{GenericPendingSource, MockPendingSourceTrait};
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
    let hash0 = stark_felt!("0x0");
    let patricia_key0 = patricia_key!("0x0");
    let hash1 = stark_felt!("0x1");
    let patricia_key1 = patricia_key!("0x1");

    let dep_contract_0 = (ContractAddress(patricia_key0), ClassHash(hash0));
    let dep_contract_1 = (ContractAddress(patricia_key1), ClassHash(hash1));
    let storage_key_0 = StorageKey(patricia_key!("0x0"));
    let storage_key_1 = StorageKey(patricia_key!("0x1"));
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

    let header_hash = BlockHash(stark_felt!("0x0"));
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
        pending_classes: Arc::new(RwLock::new(PendingClasses::new())),
        base_layer_source: Arc::new(MockBaseLayerSourceTrait::new()),
        reader,
        writer,
    };

    // Trying to store a block without a header in the storage.
    let res = gen_state_sync.store_base_layer_block(BlockNumber(1), BlockHash::default());
    assert_matches!(res, Err(StateSyncError::BaseLayerBlockWithoutMatchingHeader { .. }));

    // Trying to store a block with mismatching header.
    let res =
        gen_state_sync.store_base_layer_block(BlockNumber(0), BlockHash(stark_felt!("0x666")));
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

// TODO(dvir): move pending tests to new file.
// TODO(dvir): add test for full pending sync.
#[tokio::test]
async fn pending_sync() {
    // Storage with one default block header.
    let (reader, mut writer) = get_test_storage().0;
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();

    let mut mock_pending_source = MockPendingSourceTrait::new();

    const PENDING_QUERIES: usize = 2;
    for call_count in 0..=PENDING_QUERIES {
        mock_pending_source.expect_get_pending_data().times(1).returning(move || {
            let mut block = PendingData::default();
            block.block.gas_price = GasPrice(call_count as u128);
            Ok(block)
        });
    }

    // A different parent block hash than the last block in the database tells that a new block was
    // created, and pending sync should wait until the new block is written to the storage. so
    // this pending data should not be written.
    mock_pending_source.expect_get_pending_data().times(1).returning(|| {
        let mut block = PendingData::default();
        block.block.parent_block_hash = BlockHash(stark_felt!("0x1"));
        Ok(block)
    });

    let pending_data = Arc::new(RwLock::new(PendingData::default()));
    let pending_classes = Arc::new(RwLock::new(PendingClasses::new()));

    sync_pending_data(
        reader,
        Arc::new(mock_pending_source),
        pending_data.clone(),
        pending_classes.clone(),
        Duration::from_millis(1),
    )
    .await
    .unwrap();

    // The Last query for pending data (with parent block hash 0x1) should not be written so the gas
    // price should PENDING_QUERIES.
    assert_eq!(pending_data.read().await.block.gas_price, GasPrice(PENDING_QUERIES as u128));
}

#[tokio::test]
async fn pending_sync_add_class() {
    // Storage with one default block header.
    let (reader, mut writer) = get_test_storage().0;
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();

    let mut mock_client = MockStarknetReader::new();

    mock_client.expect_pending_data().times(1).returning(move || Ok(Some(PendingData::default())));

    let deprecated_class_hash = ClassHash(stark_felt!("0x1"));
    let contract_class_hash = ClassHash(stark_felt!("0x2"));

    // Pending data with a class hashes.
    let mut block_with_classes = PendingData::default();
    block_with_classes.state_update.state_diff.old_declared_contracts = vec![deprecated_class_hash];
    block_with_classes.state_update.state_diff.declared_classes = vec![DeclaredClassHashEntry {
        class_hash: contract_class_hash,
        compiled_class_hash: CompiledClassHash(contract_class_hash.0),
    }];

    let cloned_block = block_with_classes.clone();
    mock_client.expect_pending_data().times(1).returning(move || Ok(Some(cloned_block.clone())));

    // To make the pending sync stop.
    let mut new_pending_block = block_with_classes.clone();
    new_pending_block.block.parent_block_hash = BlockHash(stark_felt!("0x666"));
    mock_client
        .expect_pending_data()
        .times(1)
        .returning(move || Ok(Some(new_pending_block.clone())));

    mock_client
        .expect_class_by_hash()
        .times(1)
        .with(predicate::eq(deprecated_class_hash))
        .returning(move |_| {
            Ok(Some(GenericContractClass::Cairo0ContractClass(DeprecatedContractClass::default())))
        });
    mock_client.expect_class_by_hash().times(1).with(predicate::eq(contract_class_hash)).returning(
        move |_| {
            Ok(Some(GenericContractClass::Cairo1ContractClass(
                starknet_client::reader::ContractClass::default(),
            )))
        },
    );
    mock_client
        .expect_compiled_class_by_hash()
        .times(1)
        .with(predicate::eq(contract_class_hash))
        .returning(move |_| Ok(Some(CasmContractClass::default())));

    let pending_source = Arc::new(GenericPendingSource { starknet_client: Arc::new(mock_client) });
    let pending_data = Arc::new(RwLock::new(PendingData::default()));
    let pending_classes = Arc::new(RwLock::new(PendingClasses::new()));

    // Pending classes is empty.
    assert_eq!(pending_classes.read().await.deref(), &PendingClasses::new());

    sync_pending_data(
        reader,
        pending_source,
        pending_data.clone(),
        pending_classes.clone(),
        Duration::from_millis(1),
    )
    .await
    .unwrap();

    let expected_classes = HashMap::from([
        (deprecated_class_hash, PendingClass::Cairo0(DeprecatedContractClass::default())),
        (
            contract_class_hash,
            PendingClass::Cairo1(starknet_client::reader::ContractClass::default().into()),
        ),
    ]);

    let expected_casm = HashMap::from([(contract_class_hash, CasmContractClass::default())]);
    let expected = PendingClasses { classes: expected_classes, casm: expected_casm };

    assert_eq!(pending_classes.read().await.deref(), &expected);
}
