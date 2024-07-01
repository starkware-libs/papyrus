use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use lazy_static::lazy_static;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_common::state::create_random_state_diff;
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ClassQuery,
    DataOrFin,
    Direction,
    EventQuery,
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
use starknet_api::transaction::{Event, Transaction, TransactionHash, TransactionOutput};
use test_utils::{get_rng, get_test_body};

use super::{split_thin_state_diff, FetchBlockDataFromDb, P2PSyncServer};
const BUFFER_SIZE: usize = 10;
const NUM_OF_BLOCKS: u64 = 10;
const NUM_TXS_PER_BLOCK: usize = 5;
const BLOCKS_DELTA: u64 = 5;

enum StartBlockType {
    Hash,
    Number,
}

// TODO(shahak): Change tests to use channels and not register_query
#[tokio::test]
async fn header_query_positive_flow() {
    let assert_signed_block_header = |data: Vec<SignedBlockHeader>| {
        let len = data.len();
        assert!(len == NUM_OF_BLOCKS as usize);
        for (i, signed_header) in data.into_iter().enumerate() {
            assert_eq!(signed_header.block_header.block_number.0, i as u64);
        }
    };

    run_test(assert_signed_block_header, 0, StartBlockType::Hash).await;
    run_test(assert_signed_block_header, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn transaction_query_positive_flow() {
    let assert_transaction_and_output = |data: Vec<(Transaction, TransactionOutput)>| {
        let len = data.len();
        assert!(len == NUM_OF_BLOCKS as usize * NUM_TXS_PER_BLOCK);
        for (i, (tx, tx_output)) in data.into_iter().enumerate() {
            assert_eq!(tx, TXS[i / NUM_TXS_PER_BLOCK][i % NUM_TXS_PER_BLOCK]);
            assert_eq!(tx_output, TX_OUTPUTS[i / NUM_TXS_PER_BLOCK][i % NUM_TXS_PER_BLOCK]);
        }
    };

    run_test(assert_transaction_and_output, 0, StartBlockType::Hash).await;
    run_test(assert_transaction_and_output, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn state_diff_query_positive_flow() {
    let assert_state_diff_chunk = |data: Vec<StateDiffChunk>| {
        assert_eq!(data.len(), STATE_DIFF_CHUNCKS.len());

        for (data, expected_data) in data.iter().zip(STATE_DIFF_CHUNCKS.iter()) {
            assert_eq!(data, expected_data);
        }
    };
    run_test(assert_state_diff_chunk, 0, StartBlockType::Hash).await;
    run_test(assert_state_diff_chunk, 0, StartBlockType::Number).await;
}

#[tokio::test]
async fn header_query_some_blocks_are_missing() {
    let assert_signed_block_header = |data: Vec<SignedBlockHeader>| {
        let len = data.len();
        assert!(len == BLOCKS_DELTA as usize);
        for (i, signed_header) in data.into_iter().enumerate() {
            assert_eq!(
                signed_header.block_header.block_number.0,
                i as u64 + NUM_OF_BLOCKS - BLOCKS_DELTA
            );
        }
    };

    run_test(assert_signed_block_header, NUM_OF_BLOCKS - BLOCKS_DELTA, StartBlockType::Number)
        .await;
}

#[tokio::test]
async fn transaction_query_some_blocks_are_missing() {
    let assert_transaction_and_output = |data: Vec<(Transaction, TransactionOutput)>| {
        let len = data.len();
        assert!(len == (BLOCKS_DELTA as usize * NUM_TXS_PER_BLOCK));
        for (i, (tx, tx_output)) in data.into_iter().enumerate() {
            assert_eq!(
                tx,
                TXS[i / NUM_TXS_PER_BLOCK + NUM_OF_BLOCKS as usize - BLOCKS_DELTA as usize]
                    [i % NUM_TXS_PER_BLOCK]
            );
            assert_eq!(
                tx_output,
                TX_OUTPUTS[i / NUM_TXS_PER_BLOCK + NUM_OF_BLOCKS as usize - BLOCKS_DELTA as usize]
                    [i % NUM_TXS_PER_BLOCK]
            );
        }
    };

    run_test(assert_transaction_and_output, NUM_OF_BLOCKS - BLOCKS_DELTA, StartBlockType::Number)
        .await;
}

#[tokio::test]
async fn state_diff_query_some_blocks_are_missing() {
    let assert_state_diff_chunk = |data: Vec<StateDiffChunk>| {
        // create_random_state_diff creates a state diff with 5 chunks.
        const STATE_DIFF_CHUNK_PER_BLOCK: usize = 5;
        assert!(data.len() == BLOCKS_DELTA as usize * STATE_DIFF_CHUNK_PER_BLOCK);
        for (i, data) in data.into_iter().enumerate() {
            assert_eq!(
                data,
                STATE_DIFF_CHUNCKS[i
                    + (NUM_OF_BLOCKS as usize - BLOCKS_DELTA as usize)
                        * STATE_DIFF_CHUNK_PER_BLOCK]
            );
        }
    };

    run_test(assert_state_diff_chunk, NUM_OF_BLOCKS - BLOCKS_DELTA, StartBlockType::Number).await;
}

async fn run_test<T, F>(assert_fn: F, start_block_number: u64, start_block_type: StartBlockType)
where
    T: FetchBlockDataFromDb + std::fmt::Debug + PartialEq + Send + Sync + 'static,
    F: FnOnce(Vec<T>),
{
    let (
        p2p_sync_server,
        storage_reader,
        mut storage_writer,
        _header_queries_sender,
        _state_diff_queries_sender,
        _transaction_queries_sender,
        _class_queries_sender,
        _event_queries_sender,
    ) = setup();

    // put some data in the storage.
    insert_to_storage_test_blocks_up_to(&mut storage_writer);

    let start_block = match start_block_type {
        StartBlockType::Hash => BlockHashOrNumber::Hash(
            storage_reader
                .begin_ro_txn()
                .unwrap()
                .get_block_header(BlockNumber(start_block_number))
                .unwrap()
                .unwrap()
                .block_hash,
        ),
        StartBlockType::Number => BlockHashOrNumber::Number(BlockNumber(start_block_number)),
    };

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = Query { start_block, direction: Direction::Forward, limit: NUM_OF_BLOCKS, step: 1 };
    p2p_sync_server.register_query::<T, _>(query, sender);

    // run p2p_sync_server and collect query results.
    tokio::select! {
        _ = p2p_sync_server.run() => {
            panic!("p2p_sync_server should never finish its run.");
        },
        mut res = receiver.collect::<Vec<_>>() => {
            assert_eq!(DataOrFin(None), res.pop().unwrap());
            let filtered_res: Vec<T> = res.into_iter()
                    .map(|data| data.0.expect("P2PSyncServer returned Fin and then returned another response"))
                    .collect();
            assert_fn(filtered_res);
        }
    }
}

#[allow(clippy::type_complexity)]
fn setup() -> (
    P2PSyncServer<
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
        Receiver<(
            Result<ClassQuery, ProtobufConversionError>,
            Sender<DataOrFin<ApiContractClass>>,
        )>,
        Receiver<(
            Result<EventQuery, ProtobufConversionError>,
            Sender<DataOrFin<(Event, TransactionHash)>>,
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
    Sender<(Result<ClassQuery, ProtobufConversionError>, Sender<DataOrFin<ApiContractClass>>)>,
    Sender<(
        Result<EventQuery, ProtobufConversionError>,
        Sender<DataOrFin<(Event, TransactionHash)>>,
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
    let (class_sender, class_queries_receiver) = futures::channel::mpsc::channel::<(
        Result<ClassQuery, ProtobufConversionError>,
        Sender<DataOrFin<ApiContractClass>>,
    )>(BUFFER_SIZE);
    let (event_sender, event_queries_receiver) = futures::channel::mpsc::channel::<(
        Result<EventQuery, ProtobufConversionError>,
        Sender<DataOrFin<(Event, TransactionHash)>>,
    )>(BUFFER_SIZE);

    let p2p_sync_server = super::P2PSyncServer::new(
        storage_reader.clone(),
        header_queries_receiver,
        state_diff_queries_receiver,
        transaction_queries_receiver,
        class_queries_receiver,
        event_queries_receiver,
    );
    (
        p2p_sync_server,
        storage_reader,
        storage_writer,
        header_queries_sender,
        state_diff_queries_sender,
        transaction_sender,
        class_sender,
        event_sender,
    )
}

fn insert_to_storage_test_blocks_up_to(storage_writer: &mut StorageWriter) {
    for i in 0..NUM_OF_BLOCKS {
        let i_usize = usize::try_from(i).unwrap();
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
            .append_state_diff(BlockNumber(i), THIN_STATE_DIFFS[i_usize].clone())
            .unwrap()
            .append_body(BlockNumber(i), BlockBody{transactions: TXS[i_usize].clone(),
                transaction_outputs: TX_OUTPUTS[i_usize].clone(),
                transaction_hashes: TX_HASHES[i_usize].clone(),}).unwrap()
            .commit()
            .unwrap();
    }
}

lazy_static! {
    static ref THIN_STATE_DIFFS: Vec<starknet_api::state::ThinStateDiff> = {
        let mut rng = get_rng();
        (0..NUM_OF_BLOCKS).map(|_| create_random_state_diff(&mut rng)).collect::<Vec<_>>()
    };
    static ref STATE_DIFF_CHUNCKS: Vec<StateDiffChunk> =
        THIN_STATE_DIFFS.iter().flat_map(|diff| split_thin_state_diff(diff.clone())).collect();
    static ref BODY: BlockBody =
        get_test_body(NUM_OF_BLOCKS as usize * NUM_TXS_PER_BLOCK, None, None, None);
    static ref TXS: Vec<Vec<Transaction>> =
        BODY.clone().transactions.chunks(NUM_TXS_PER_BLOCK).map(|chunk| chunk.to_vec()).collect();
    static ref TX_OUTPUTS: Vec<Vec<TransactionOutput>> = BODY
        .clone()
        .transaction_outputs
        .chunks(NUM_TXS_PER_BLOCK)
        .map(|chunk| chunk.to_vec())
        .collect();
    static ref TX_HASHES: Vec<Vec<TransactionHash>> = BODY
        .clone()
        .transaction_hashes
        .chunks(NUM_TXS_PER_BLOCK)
        .map(|chunk| chunk.to_vec())
        .collect();
    static ref EVENTS: Vec<Event> = TX_OUTPUTS
        .clone()
        .into_iter()
        .flat_map(|tx_output| tx_output.into_iter().flat_map(|output| output.events().to_vec()))
        .collect();
}
