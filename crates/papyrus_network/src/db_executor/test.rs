use futures::channel::mpsc::Receiver;
use futures::stream::SelectAll;
use futures::{FutureExt, StreamExt};
use papyrus_common::state::create_random_state_diff;
use papyrus_protobuf::sync::{BlockHashOrNumber, Direction, Query, SignedBlockHeader};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use rand::random;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use test_utils::get_rng;

use crate::db_executor::{DBExecutorError, DBExecutorTrait, Data, MockFetchBlockDataFromDb};
use crate::DataType;

const BUFFER_SIZE: usize = 10;

#[tokio::test]
async fn header_db_executor_can_register_and_run_a_query() {
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
    type ReceiversType = Vec<(Receiver<Vec<Data>>, DataType)>;
    let mut receivers: ReceiversType = enum_iterator::all::<DataType>()
        .map(|data_type| {
            let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
            db_executor.register_query(query.clone(), data_type, sender);
            (receiver, data_type)
        })
        .collect();
    let mut receivers_stream = SelectAll::new();
    receivers
        .iter_mut()
        .map(|(receiver, requested_data_type)| {
            receiver
                .collect::<Vec<_>>()
                .map(|collected| async move { (collected, requested_data_type) })
        })
        .for_each(|fut| {
            receivers_stream.push(fut.into_stream());
        });

    // run the executor and collect query results.
    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        _ = async {
            while let Some(res) = receivers_stream.next().await {
                let (data, requested_data_type) = res.await;
                assert_eq!(data.len(), NUM_OF_BLOCKS as usize);
                for (i, data) in data.into_iter().enumerate() {
                    for data in data.iter() {
                        match data {
                            Data::BlockHeaderAndSignature(SignedBlockHeader { block_header: BlockHeader { block_number: BlockNumber(block_number), .. }, .. }) => {
                                assert_eq!(block_number, &(i as u64));
                                assert_eq!(*requested_data_type, DataType::SignedBlockHeader);
                            }
                            Data::StateDiffChunk (_state_diff)  => {
                                // TODO: check the state diff.
                                assert_eq!(*requested_data_type, DataType::StateDiff);
                            }
                            _ => panic!("Unexpected data type"),
                        }
                    }
                }
            }
        } => {}
    }
}

#[tokio::test]
async fn header_db_executor_start_block_given_by_hash() {
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
    db_executor.register_query(query, DataType::SignedBlockHeader, sender);

    // run the executor and collect query results.
    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), NUM_OF_BLOCKS as usize);
            for (i, data) in res.into_iter().enumerate() {
                for data in data.iter() {
                    match data {
                        Data::BlockHeaderAndSignature(
                            SignedBlockHeader {
                                block_header: BlockHeader {
                                    block_number: BlockNumber(block_number),
                                    ..
                                },
                                ..
                            }) => {
                            assert_eq!(block_number, &(i as u64));
                        }
                        _ => panic!("Unexpected data type"),
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn header_db_executor_query_of_missing_block() {
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
    let mut mock_data_type = MockFetchBlockDataFromDb::new();
    mock_data_type.expect_fetch_block_data_from_db().times((BLOCKS_DELTA + 1) as usize).returning(
        |block_number, query_id, _| {
            if block_number.0 == NUM_OF_BLOCKS {
                Err(DBExecutorError::BlockNotFound {
                    block_hash_or_number: BlockHashOrNumber::Number(block_number),
                    query_id,
                })
            } else {
                Ok(vec![Data::default()])
            }
        },
    );
    let _query_id = db_executor.register_query(query, mock_data_type, sender);

    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), (BLOCKS_DELTA) as usize);
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
