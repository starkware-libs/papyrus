use std::time::Duration;

use futures::channel::mpsc::{Receiver, Sender};
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use papyrus_network::{DataType, Direction, Query, ResponseReceivers, SignedBlockHeader};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::crypto::Signature;
use starknet_api::hash::{StarkFelt, StarkHash};
use tokio::time::timeout;

use super::{P2PSync, P2PSyncConfig};

const BUFFER_SIZE: usize = 1000;
const QUERY_LENGTH: usize = 5;
const DURATION_BEFORE_CHECKING_STORAGE: Duration = Duration::from_millis(10);
const QUERY_TIMEOUT: Duration = Duration::from_millis(50);
const TIMEOUT_AFTER_QUERY_TIMEOUTED_IN_SYNC: Duration = QUERY_TIMEOUT.saturating_mul(5);

lazy_static! {
    static ref TEST_CONFIG: P2PSyncConfig =
        P2PSyncConfig { num_headers_per_query: QUERY_LENGTH, query_timeout: QUERY_TIMEOUT };
}

fn setup() -> (P2PSync, StorageReader, Receiver<Query>, Sender<Option<SignedBlockHeader>>) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (query_sender, query_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let (signed_headers_sender, signed_headers_receiver) =
        futures::channel::mpsc::channel(BUFFER_SIZE);
    let p2p_sync = P2PSync::new(
        *TEST_CONFIG,
        storage_reader.clone(),
        storage_writer,
        query_sender,
        ResponseReceivers { signed_headers_receiver: signed_headers_receiver.boxed() },
    );
    (p2p_sync, storage_reader, query_receiver, signed_headers_sender)
}

fn create_block_hashes_and_signatures(n_blocks: u8) -> Vec<(BlockHash, BlockSignature)> {
    let mut bytes = [0u8; 32];
    (0u8..n_blocks)
        .map(|i| {
            bytes[31] = i;
            (
                BlockHash(StarkHash::new(bytes).unwrap()),
                BlockSignature(Signature {
                    r: StarkFelt::new(bytes).unwrap(),
                    s: StarkFelt::new(bytes).unwrap(),
                }),
            )
        })
        .collect()
}

#[tokio::test]
async fn signed_headers_basic_flow() {
    const NUM_QUERIES: usize = 3;

    let (p2p_sync, storage_reader, mut query_receiver, mut signed_headers_sender) = setup();
    let block_hashes_and_signatures =
        create_block_hashes_and_signatures((NUM_QUERIES * QUERY_LENGTH).try_into().unwrap());

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        for query_index in 0..NUM_QUERIES {
            let start_block_number = query_index * QUERY_LENGTH;
            let end_block_number = (query_index + 1) * QUERY_LENGTH;

            // Receive query and validate it.
            let query = query_receiver.next().await.unwrap();
            assert_eq!(
                query,
                Query {
                    start_block: BlockNumber(start_block_number.try_into().unwrap()),
                    direction: Direction::Forward,
                    limit: QUERY_LENGTH,
                    step: 1,
                    data_type: DataType::SignedBlockHeader,
                }
            );

            // Send responses
            for (i, (block_hash, block_signature)) in block_hashes_and_signatures
                .iter()
                .enumerate()
                .take(end_block_number)
                .skip(start_block_number)
            {
                signed_headers_sender
                    .send(Some(SignedBlockHeader {
                        block_header: BlockHeader {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            block_hash: *block_hash,
                            ..Default::default()
                        },
                        signatures: vec![*block_signature],
                    }))
                    .await
                    .unwrap();
            }

            tokio::time::sleep(DURATION_BEFORE_CHECKING_STORAGE).await;

            // Check responses were written to the storage.
            let txn = storage_reader.begin_ro_txn().unwrap();
            assert_eq!(
                u64::try_from(end_block_number).unwrap(),
                txn.get_header_marker().unwrap().0
            );

            for (i, (block_hash, block_signature)) in block_hashes_and_signatures
                .iter()
                .enumerate()
                .take(end_block_number)
                .skip(start_block_number)
            {
                let block_number = BlockNumber(i.try_into().unwrap());
                let block_header = txn.get_block_header(block_number).unwrap().unwrap();
                assert_eq!(block_number, block_header.block_number);
                assert_eq!(*block_hash, block_header.block_hash);
                let actual_block_signature =
                    txn.get_block_signature(block_number).unwrap().unwrap();
                assert_eq!(*block_signature, actual_block_signature);
            }
        }
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}

#[tokio::test]
async fn sync_sends_new_query_if_it_got_partial_responses() {
    const NUM_ACTUAL_RESPONSES: u8 = 2;
    assert!(usize::from(NUM_ACTUAL_RESPONSES) < QUERY_LENGTH);

    let (p2p_sync, _storage_reader, mut query_receiver, mut signed_headers_sender) = setup();
    let block_hashes_and_signatures = create_block_hashes_and_signatures(NUM_ACTUAL_RESPONSES);

    // Create a future that will receive a query, send partial responses and receive the next query.
    let parse_queries_future = async move {
        let _query = query_receiver.next().await.unwrap();

        for (i, (block_hash, signature)) in block_hashes_and_signatures.into_iter().enumerate() {
            signed_headers_sender
                .send(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_number: BlockNumber(i.try_into().unwrap()),
                        block_hash,
                        ..Default::default()
                    },
                    signatures: vec![signature],
                }))
                .await
                .unwrap();
        }

        // First unwrap is for the timeout. Second unwrap is for the Option returned from Stream.
        let query = timeout(TIMEOUT_AFTER_QUERY_TIMEOUTED_IN_SYNC, query_receiver.next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            query,
            Query {
                start_block: BlockNumber(NUM_ACTUAL_RESPONSES.into()),
                direction: Direction::Forward,
                limit: QUERY_LENGTH,
                step: 1,
                data_type: DataType::SignedBlockHeader,
            }
        );
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}

// TODO(shahak): Add negative tests.
