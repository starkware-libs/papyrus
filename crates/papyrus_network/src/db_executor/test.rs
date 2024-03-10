use std::task::Poll;

use assert_matches::assert_matches;
use futures::channel::mpsc::Receiver;
use futures::future::poll_fn;
use futures::stream::SelectAll;
use futures::{FutureExt, StreamExt};
use indexmap::IndexMap;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use rand::random;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::state::{StateDiff, ThinStateDiff};

use super::Data::BlockHeaderAndSignature;
use crate::db_executor::{DBExecutor, DBExecutorError, Data, MockFetchBlockDataFromDb, QueryId};
use crate::{BlockHashOrNumber, DataType, Direction, InternalQuery};
const BUFFER_SIZE: usize = 10;

#[tokio::test]
async fn header_db_executor_can_register_and_run_a_query() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // put some data in the storage.
    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    // register a query.
    let query = InternalQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(0)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let (query_ids, mut receivers): (Vec<QueryId>, Vec<(Receiver<Data>, DataType)>) =
        enum_iterator::all::<DataType>()
            .map(|data_type| {
                let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
                let query_id = db_executor.register_query(query, data_type, sender);
                (query_id, (receiver, data_type))
            })
            .unzip();
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
        res = db_executor.next() => {
            let poll_res = res.unwrap();
            let res_query_id = poll_res.unwrap();
            assert!(query_ids.iter().any(|query_id| query_id == &res_query_id));
        }
        Some(res) = receivers_stream.next() => {
            let (data, requested_data_type) = res.await;
            assert_eq!(data.len(), NUM_OF_BLOCKS as usize);
            for (i, data) in data.iter().enumerate() {
                if i == 0{
                    // requested DataType dictates what kind of Data we should expect.
                    match requested_data_type {
                        DataType::SignedBlockHeader => {
                            assert_matches!(data, BlockHeaderAndSignature { .. });
                        }
                        DataType::StateDiff => {
                            assert_matches!(data, Data::StateDiff{..});

                        }
                    }
                }
                match data {
                    Data::BlockHeaderAndSignature { header: BlockHeader { block_number: BlockNumber(block_number), .. }, ..} => {
                        assert_eq!(block_number, &(i as u64));
                    }
                    Data::StateDiff{state_diff: ThinStateDiff { .. }} => {
                        // TODO: check the state diff.
                    }
                    _ => panic!("Unexpected data type"),
                }
            }
        }
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

    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = InternalQuery {
        start_block: BlockHashOrNumber::Hash(block_hash),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let query_id = db_executor.register_query(query, DataType::SignedBlockHeader, sender);

    // run the executor and collect query results.
    tokio::select! {
        res = db_executor.next() => {
            let poll_res = res.unwrap();
            let res_query_id = poll_res.unwrap();
            assert_eq!(res_query_id, query_id);
        }
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), NUM_OF_BLOCKS as usize);
            for (i, data) in res.iter().enumerate() {
                assert_matches!(data, BlockHeaderAndSignature { header: BlockHeader { block_number: BlockNumber(block_number), .. }, ..} if block_number == &(i as u64));
            }
        }
    }
}
#[tokio::test]
async fn header_db_executor_query_of_missing_block() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 15;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    const BLOCKS_DELTA: u64 = 5;
    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = InternalQuery {
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
                Ok(Data::default())
            }
        },
    );
    let _query_id = db_executor.register_query(query, mock_data_type, sender);

    tokio::select! {
        res = db_executor.next() => {
            let poll_res = res.unwrap();
            assert_matches!(poll_res, Err(DBExecutorError::BlockNotFound{..}));
        }
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), (BLOCKS_DELTA) as usize);
        }
    }
}

#[test]
fn header_db_executor_stream_pending_with_no_query() {
    let ((storage_reader, _), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // poll without registering a query.
    assert!(poll_fn(|cx| db_executor.poll_next_unpin(cx)).now_or_never().is_none());
}

#[tokio::test]
async fn header_db_executor_can_receive_queries_after_stream_is_exhausted() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    for _ in 0..2 {
        // register a query.
        let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
        let query = InternalQuery {
            start_block: BlockHashOrNumber::Number(BlockNumber(0)),
            direction: Direction::Forward,
            limit: NUM_OF_BLOCKS,
            step: 1,
        };
        let mut mock_data_type = MockFetchBlockDataFromDb::new();
        mock_data_type
            .expect_fetch_block_data_from_db()
            .times(NUM_OF_BLOCKS as usize)
            .returning(|_, _, _| Ok(Data::default()));
        let query_id = db_executor.register_query(query, mock_data_type, sender);

        // run the executor and collect query results.
        receiver.collect::<Vec<_>>().await;
        let res = db_executor.next().await;
        assert_eq!(res.unwrap().unwrap(), query_id);

        // make sure the stream is pending.
        let res = poll_fn(|cx| match db_executor.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(Ok(())),
            Poll::Ready(ready) => Poll::Ready(Err(ready)),
        })
        .await;
        assert!(res.is_ok());
    }
}

#[tokio::test]
async fn header_db_executor_drop_receiver_before_query_is_done() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = InternalQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(1)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    drop(receiver);

    // register a query.
    let _query_id = db_executor.register_query(query, MockFetchBlockDataFromDb::new(), sender);

    // executor should return an error.
    let res = db_executor.next().await;
    assert!(res.unwrap().is_err());
}

fn insert_to_storage_test_blocks_up_to(num_of_blocks: u64, storage_writer: &mut StorageWriter) {
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
            .append_state_diff(BlockNumber(i), StateDiff::default(),IndexMap::new())
            .unwrap()
            .commit()
            .unwrap();
    }
}
