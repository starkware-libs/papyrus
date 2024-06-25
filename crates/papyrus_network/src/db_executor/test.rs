use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use lazy_static::lazy_static;
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
    TransactionQuery,
};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageReader, StorageWriter};
use rand::random;
use starknet_api::block::{BlockBody, BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::transaction::{Transaction, TransactionOutput};
use test_utils::{get_rng, get_test_body};

use super::{DBExecutor, FetchBlockDataFromDb};
const BUFFER_SIZE: usize = 10;
const NUM_OF_BLOCKS: u64 = 10;

// TODO: Add test for state_diff.
// TODO(shahak): Change tests to use channels and not register_query
#[tokio::test]
async fn header_transaction_query_positive_flow() {
    fn assert_signed_block_header(i: usize, data: Option<SignedBlockHeader>, len: usize) {
        match data {
            Some(signed_header) => {
                assert_eq!(signed_header.block_header.block_number.0, i as u64);
            }
            None => assert_eq!(i, len),
        };
    }

    fn assert_transaction_and_output(
        i: usize,
        data: Option<(Transaction, TransactionOutput)>,
        len: usize,
    ) {
        match data {
            Some((tx, tx_output)) => {
                assert!(tx == TXS[i] && tx_output == TX_OUTPUTS[i]);
            }
            None => assert_eq!(i, len),
        };
    }
    run_query_start_block_given_by_hash::<SignedBlockHeader, _>(assert_signed_block_header, true)
        .await;
    run_query_start_block_given_by_hash::<(Transaction, TransactionOutput), _>(
        assert_transaction_and_output,
        true,
    )
    .await;
    run_query_start_block_given_by_hash::<SignedBlockHeader, _>(assert_signed_block_header, false)
        .await;
    run_query_start_block_given_by_hash::<(Transaction, TransactionOutput), _>(
        assert_transaction_and_output,
        false,
    )
    .await;
}

async fn run_query_start_block_given_by_hash<T, F>(assert_fn: F, by_hash: bool)
where
    T: FetchBlockDataFromDb + PartialEq + Send + Sync + 'static,
    F: Fn(usize, Option<T>, usize),
{
    let (
        db_executor,
        storage_reader,
        mut storage_writer,
        _header_queries_sender,
        _state_diff_queries_sender,
        _transaction_queries_sender,
    ) = setup();

    // put some data in the storage.
    insert_to_storage_test_blocks_up_to(&mut storage_writer);

    let start_block = if by_hash {
        BlockHashOrNumber::Hash(
            storage_reader
                .begin_ro_txn()
                .unwrap()
                .get_block_header(BlockNumber(0))
                .unwrap()
                .unwrap()
                .block_hash,
        )
    } else {
        BlockHashOrNumber::Number(BlockNumber(0))
    };

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = Query { start_block, direction: Direction::Forward, limit: NUM_OF_BLOCKS, step: 1 };
    db_executor.register_query::<T, _>(query, sender);

    // run the executor and collect query results.
    tokio::select! {
        _ = db_executor.run() => {
            panic!("DB executor should never finish its run.");
        },
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), NUM_OF_BLOCKS as usize + 1);
            for (i, data) in res.into_iter().enumerate() {
                assert_fn(i, data.0, NUM_OF_BLOCKS as usize );
            }
        }
    }
}

#[tokio::test]
async fn header_transaction_query_some_blocks_are_missing() {
    run_query_some_blocks_are_missing::<(Transaction, TransactionOutput)>().await;
    run_query_some_blocks_are_missing::<SignedBlockHeader>().await;
}

async fn run_query_some_blocks_are_missing<T>()
where
    T: FetchBlockDataFromDb + Send + Sync + 'static + std::fmt::Debug + PartialEq,
{
    let (
        db_executor,
        _storage_reader,
        mut storage_writer,
        _header_queries_sender,
        _state_diff_queries_sender,
        _transaction_queries_sender,
    ) = setup();

    insert_to_storage_test_blocks_up_to(&mut storage_writer);

    const BLOCKS_DELTA: u64 = 5;
    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(NUM_OF_BLOCKS - BLOCKS_DELTA)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    db_executor.register_query::<T, _>(query, sender);

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
        Receiver<(
            Result<TransactionQuery, ProtobufConversionError>,
            Sender<DataOrFin<(Transaction, TransactionOutput)>>,
        )>,
    >,
    StorageReader,
    StorageWriter,
    Sender<(Result<HeaderQuery, ProtobufConversionError>, Sender<DataOrFin<SignedBlockHeader>>)>,
    Sender<(Result<StateDiffQuery, ProtobufConversionError>, Sender<DataOrFin<StateDiffChunk>>)>,
    Sender<(
        Result<TransactionQuery, ProtobufConversionError>,
        Sender<DataOrFin<(Transaction, TransactionOutput)>>,
    )>,
) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (header_queries_sender, header_queries_receiver) = futures::channel::mpsc::channel::<(
        Result<HeaderQuery, ProtobufConversionError>,
        Sender<DataOrFin<SignedBlockHeader>>,
    )>(BUFFER_SIZE);
    let (state_diff_queries_sender, state_diff_queries_receiver) = futures::channel::mpsc::channel::<
        (Result<StateDiffQuery, ProtobufConversionError>, Sender<DataOrFin<StateDiffChunk>>),
    >(BUFFER_SIZE);
    let (transaction_sender, transaction_queries_receiver) = futures::channel::mpsc::channel::<(
        Result<TransactionQuery, ProtobufConversionError>,
        Sender<DataOrFin<(Transaction, TransactionOutput)>>,
    )>(BUFFER_SIZE);

    let db_executor = super::DBExecutor::new(
        storage_reader.clone(),
        header_queries_receiver,
        state_diff_queries_receiver,
        transaction_queries_receiver,
    );
    (
        db_executor,
        storage_reader,
        storage_writer,
        header_queries_sender,
        state_diff_queries_sender,
        transaction_sender,
    )
}

fn insert_to_storage_test_blocks_up_to(storage_writer: &mut StorageWriter) {
    let mut rng = get_rng();
    let thin_state_diffs =
        (0..NUM_OF_BLOCKS).map(|_| create_random_state_diff(&mut rng)).collect::<Vec<_>>();

    for i in 0..NUM_OF_BLOCKS {
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
            .append_body(BlockNumber(i), BlockBody{transactions: vec![TXS[i as usize].clone()],
                transaction_outputs: vec![TX_OUTPUTS[i as usize].clone()],
                transaction_hashes: vec![TX_HASHES[i as usize]],}).unwrap()
            .commit()
            .unwrap();
    }
}

lazy_static! {
    static ref BODY: BlockBody = get_test_body(NUM_OF_BLOCKS as usize, None, None, None);
    static ref TXS: Vec<Transaction> = BODY.clone().transactions;
    static ref TX_OUTPUTS: Vec<TransactionOutput> = BODY.clone().transaction_outputs;
    static ref TX_HASHES: Vec<starknet_api::transaction::TransactionHash> =
        BODY.clone().transaction_hashes;
}
