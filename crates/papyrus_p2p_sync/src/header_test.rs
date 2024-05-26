use futures::future::ready;
use futures::{SinkExt, StreamExt};
use papyrus_network::{BlockHashOrNumber, DataType, Direction, Query, SignedBlockHeader};
use papyrus_storage::header::HeaderStorageReader;
use starknet_api::block::{BlockHeader, BlockNumber};
use tokio::time::timeout;

use crate::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE,
};

#[tokio::test]
async fn signed_headers_basic_flow() {
    const NUM_QUERIES: u64 = 3;

    let (p2p_sync, storage_reader, query_receiver, mut signed_headers_sender, _state_diffs_sender) =
        setup();
    let block_hashes_and_signatures =
        create_block_hashes_and_signatures((NUM_QUERIES * HEADER_QUERY_LENGTH).try_into().unwrap());

    let mut query_receiver = query_receiver
        .filter(|(_query, data_type)| ready(matches!(data_type, DataType::SignedBlockHeader)));

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        for query_index in 0..NUM_QUERIES {
            let start_block_number = query_index * HEADER_QUERY_LENGTH;
            let end_block_number = (query_index + 1) * HEADER_QUERY_LENGTH;

            // Receive query and validate it.
            let (query, _) = query_receiver.next().await.unwrap();
            assert_eq!(
                query,
                Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit: HEADER_QUERY_LENGTH,
                    step: 1,
                }
            );

            for (i, (block_hash, block_signature)) in block_hashes_and_signatures
                .iter()
                .enumerate()
                .take(end_block_number.try_into().expect("Failed converting u64 to usize"))
                .skip(start_block_number.try_into().expect("Failed converting u64 to usize"))
            {
                // Send responses
                signed_headers_sender
                    .send(Some(SignedBlockHeader {
                        block_header: BlockHeader {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            block_hash: *block_hash,
                            state_diff_length: Some(0),
                            ..Default::default()
                        },
                        signatures: vec![*block_signature],
                    }))
                    .await
                    .unwrap();

                tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

                // Check responses were written to the storage. This way we make sure that the sync
                // writes to the storage each response it receives before all query responses were
                // sent.
                let block_number = BlockNumber(i.try_into().unwrap());
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number.unchecked_next(), txn.get_header_marker().unwrap());
                let block_header = txn.get_block_header(block_number).unwrap().unwrap();
                assert_eq!(block_number, block_header.block_number);
                assert_eq!(*block_hash, block_header.block_hash);
                let actual_block_signature =
                    txn.get_block_signature(block_number).unwrap().unwrap();
                assert_eq!(*block_signature, actual_block_signature);
            }
            signed_headers_sender.send(None).await.unwrap();
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
async fn sync_sends_new_header_query_if_it_got_partial_responses() {
    const NUM_ACTUAL_RESPONSES: u8 = 2;
    assert!(u64::from(NUM_ACTUAL_RESPONSES) < HEADER_QUERY_LENGTH);

    let (p2p_sync, _storage_reader, query_receiver, mut signed_headers_sender, _state_diffs_sender) =
        setup();
    let block_hashes_and_signatures = create_block_hashes_and_signatures(NUM_ACTUAL_RESPONSES);

    let mut query_receiver = query_receiver
        .filter(|(_query, data_type)| ready(matches!(data_type, DataType::SignedBlockHeader)));

    // Create a future that will receive a query, send partial responses and receive the next query.
    let parse_queries_future = async move {
        let _query = query_receiver.next().await.unwrap();

        for (i, (block_hash, signature)) in block_hashes_and_signatures.into_iter().enumerate() {
            signed_headers_sender
                .send(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_number: BlockNumber(i.try_into().unwrap()),
                        block_hash,
                        state_diff_length: Some(0),
                        ..Default::default()
                    },
                    signatures: vec![signature],
                }))
                .await
                .unwrap();
        }
        signed_headers_sender.send(None).await.unwrap();

        // First unwrap is for the timeout. Second unwrap is for the Option returned from Stream.
        let (query, _) =
            timeout(TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE, query_receiver.next())
                .await
                .unwrap()
                .unwrap();

        assert_eq!(
            query,
            Query {
                start_block: BlockHashOrNumber::Number(BlockNumber(NUM_ACTUAL_RESPONSES.into())),
                direction: Direction::Forward,
                limit: HEADER_QUERY_LENGTH,
                step: 1,
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
