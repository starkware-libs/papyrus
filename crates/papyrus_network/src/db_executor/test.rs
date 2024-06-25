use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use papyrus_common::state::create_random_state_diff;
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    Direction,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageReader, StorageWriter};
use rand::random;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use test_utils::get_rng;

use super::DBExecutor;

const BUFFER_SIZE: usize = 10;

// TODO(shahak): Add test for state_diff_query_positive_flow.
// TODO(shahak): Change tests to use channels and not register_query
#[tokio::test]
async fn header_query_positive_flow() {
    let (
        db_executor,
        _storage_reader,
        mut storage_writer,
        _header_queries_sender,
        _state_diff_queries_sender,
    ) = setup();

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
    db_executor.register_query::<SignedBlockHeader, _>(query.clone(), sender);

    // run the executor and collect query results.
    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        mut all_data = data_receiver.collect::<Vec<_>>() => {
            assert_eq!(all_data.len(), NUM_OF_BLOCKS as usize + 1);
            assert_eq!(DataOrFin(None), all_data.pop().unwrap());
            for (i, data) in all_data.into_iter().enumerate() {
                assert_eq!(
                    data.0.expect("Received fin too early.").block_header.block_number.0,
                    i as u64
                );
            }
        }
    }
}

#[tokio::test]
async fn header_query_start_block_given_by_hash() {
    let (
        db_executor,
        storage_reader,
        mut storage_writer,
        _header_queries_sender,
        _state_diff_queries_sender,
    ) = setup();

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

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = Query {
        start_block: BlockHashOrNumber::Hash(block_hash),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    db_executor.register_query::<SignedBlockHeader, _>(query, sender);

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
    let (
        db_executor,
        _storage_reader,
        mut storage_writer,
        _header_queries_sender,
        _state_diff_queries_sender,
    ) = setup();

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
    db_executor.register_query::<SignedBlockHeader, _>(query, sender);

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

#[allow(clippy::type_complexity)]
fn setup() -> (
    DBExecutor<
        Receiver<(
            Result<HeaderQuery, ProtobufConversionError>,
            Sender<DataOrFin<SignedBlockHeader>>,
        )>,
        Receiver<(
            Result<StateDiffQuery, ProtobufConversionError>,
            Sender<DataOrFin<StateDiffChunk>>,
        )>,
    >,
    StorageReader,
    StorageWriter,
    Sender<(Result<HeaderQuery, ProtobufConversionError>, Sender<DataOrFin<SignedBlockHeader>>)>,
    Sender<(Result<StateDiffQuery, ProtobufConversionError>, Sender<DataOrFin<StateDiffChunk>>)>,
) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (header_queries_sender, header_queries_receiver) = futures::channel::mpsc::channel::<(
        Result<HeaderQuery, ProtobufConversionError>,
        Sender<DataOrFin<SignedBlockHeader>>,
    )>(BUFFER_SIZE);
    let (state_diff_queries_sender, state_diff_queries_receiver) = futures::channel::mpsc::channel::<
        (Result<StateDiffQuery, ProtobufConversionError>, Sender<DataOrFin<StateDiffChunk>>),
    >(BUFFER_SIZE);
    let db_executor = super::DBExecutor::new(
        storage_reader.clone(),
        header_queries_receiver,
        state_diff_queries_receiver,
    );
    (db_executor, storage_reader, storage_writer, header_queries_sender, state_diff_queries_sender)
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
