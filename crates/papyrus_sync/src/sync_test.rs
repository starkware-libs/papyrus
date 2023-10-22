use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use futures_util::StreamExt;
use indexmap::IndexMap;
use papyrus_storage::base_layer::BaseLayerStorageReader;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageReader, StorageWriter};
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash, GENESIS_HASH};
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_api::{patricia_key, stark_felt};
use starknet_client::reader::objects::pending_data::PendingBlock;
use starknet_client::reader::objects::transaction::Transaction as ClientTransaction;
use starknet_client::reader::PendingData;
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

async fn test_pending_sync(
    reader: StorageReader,
    old_pending_data: PendingData,
    new_pending_datas: Vec<PendingData>,
    expected_pending_data: PendingData,
    non_existing_hash: BlockHash,
) {
    let mut mock_pending_source = MockPendingSourceTrait::new();
    let pending_data_lock = Arc::new(RwLock::new(old_pending_data));

    for new_pending_data in new_pending_datas {
        mock_pending_source
            .expect_get_pending_data()
            .times(1)
            .return_once(move || Ok(new_pending_data));
    }

    // The syncing will stop once we see a new parent_block_hash in the pending data. It won't
    // store the pending data with the new hash in that case.
    mock_pending_source.expect_get_pending_data().times(1).return_once(move || {
        Ok(PendingData {
            block: PendingBlock { parent_block_hash: non_existing_hash, ..Default::default() },
            ..Default::default()
        })
    });

    sync_pending_data(
        reader,
        Arc::new(mock_pending_source),
        pending_data_lock.clone(),
        Duration::ZERO,
    )
    .await
    .unwrap();

    assert_eq!(pending_data_lock.read().await.clone(), expected_pending_data);
}

#[tokio::test]
async fn pending_sync_advances_only_when_new_data_has_more_transactions() {
    let genesis_hash = BlockHash(stark_felt!(GENESIS_HASH));
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
    let expected_pending_data = advanced_pending_data.clone();
    test_pending_sync(
        reader,
        old_pending_data,
        vec![advanced_pending_data, less_advanced_pending_data],
        expected_pending_data,
        BlockHash(StarkHash::ONE),
    )
    .await
}

#[tokio::test]
async fn pending_sync_new_data_has_more_advanced_hash_and_less_transactions() {
    const FIRST_BLOCK_HASH: BlockHash = BlockHash(StarkHash::ONE);
    let genesis_hash = BlockHash(stark_felt!(GENESIS_HASH));
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
    let expected_pending_data = new_pending_data.clone();
    test_pending_sync(
        reader,
        old_pending_data,
        vec![new_pending_data],
        expected_pending_data,
        BlockHash(StarkHash::TWO),
    )
    .await
}
