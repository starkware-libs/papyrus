use futures::StreamExt;
use papyrus_common::state::create_random_state_diff;
use papyrus_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query, SignedBlockHeader};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use rand::random;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use test_utils::get_rng;

use crate::db_executor::DBExecutorTrait;

const BUFFER_SIZE: usize = 10;

// TODO(shahak): Add test for state_diff_query_positive_flow.
#[tokio::test]
async fn header_query_positive_flow() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::DBExecutor::new(storage_reader);

    // put some data in the storage.
    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    // register a query.
    let query = Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(0)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let (sender, data_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    db_executor.register_query::<SignedBlockHeader>(query.clone(), sender);

    // run the executor and collect query results.
    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        all_data = data_receiver.collect::<Vec<_>>() => {
            let len = all_data.len();
            assert_eq!(len, NUM_OF_BLOCKS as usize + 1);
            for (i, data) in all_data.into_iter().enumerate() {
                match data {
                    DataOrFin(Some(signed_header)) => {
                        assert_eq!(signed_header.block_header.block_number.0, i as u64);
                    }
                    DataOrFin(None) => assert_eq!(i, len - 1),
                }
            }
        }
    }
}

#[tokio::test]
async fn header_query_start_block_given_by_hash() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();

    // put some data in the storage.
    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    let block_hash = storage_reader
        .begin_ro_txn()
        .unwrap()
        .get_block_header(BlockNumber(0))
        .unwrap()
        .unwrap()
        .block_hash;

    let mut db_executor = super::DBExecutor::new(storage_reader);

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = Query {
        start_block: BlockHashOrNumber::Hash(block_hash),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    db_executor.register_query::<SignedBlockHeader>(query, sender);

    // run the executor and collect query results.
    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        res = receiver.collect::<Vec<_>>() => {
            let len = res.len();
            assert_eq!(len, NUM_OF_BLOCKS as usize + 1);
            for (i, data) in res.into_iter().enumerate() {
                match data {
                    DataOrFin(Some(signed_header)) => {
                        assert_eq!(signed_header.block_header.block_number.0, i as u64);
                    }
                    DataOrFin(None) => assert_eq!(i, len - 1),
                };
            }
        }
    }
}

#[tokio::test]
async fn header_query_some_blocks_are_missing() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::DBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 15;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    const BLOCKS_DELTA: u64 = 5;
    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(NUM_OF_BLOCKS - BLOCKS_DELTA)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    db_executor.register_query::<SignedBlockHeader>(query, sender);

    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), (BLOCKS_DELTA + 1) as usize);
            for (i, data) in res.into_iter().enumerate() {
                assert_eq!(i == usize::try_from(BLOCKS_DELTA).unwrap(), data.0.is_none());
            }
        }
    }
}

fn insert_to_storage_test_blocks_up_to(num_of_blocks: u64, storage_writer: &mut StorageWriter) {
    let mut rng = get_rng();
    let thin_state_diffs =
        (0..num_of_blocks).map(|_| create_random_state_diff(&mut rng)).collect::<Vec<_>>();

    for i in 0..num_of_blocks {
        let block_header = BlockHeader {
            block_number: BlockNumber(i),
            block_hash: BlockHash(random::<u64>().into()),
            ..Default::default()
        };
        storage_writer
            .begin_rw_txn()
            .unwrap()
            .append_header(BlockNumber(i), &block_header)
            .unwrap()
            // TODO(shahak): Put different signatures for each block to test that we retrieve the
            // right signatures.
            .append_block_signature(BlockNumber(i), &BlockSignature::default())
            .unwrap()
            .append_state_diff(BlockNumber(i), thin_state_diffs[i as usize].clone())
            .unwrap()
            .commit()
            .unwrap();
    }
}
