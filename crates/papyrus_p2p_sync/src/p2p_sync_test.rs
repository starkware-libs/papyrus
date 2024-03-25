use std::time::Duration;

use futures::channel::mpsc::{Receiver, Sender};
use futures::future::ready;
use futures::{FutureExt, SinkExt, StreamExt};
use indexmap::indexmap;
use lazy_static::lazy_static;
use papyrus_network::{DataType, Direction, Query, ResponseReceivers, SignedBlockHeader};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use rand::RngCore;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::crypto::Signature;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::ThinStateDiff;
use static_assertions::const_assert;
use test_utils::get_rng;
use tokio::time::timeout;

use super::{P2PSync, P2PSyncConfig};

const BUFFER_SIZE: usize = 1000;
const HEADER_QUERY_LENGTH: usize = 5;
const STATE_DIFF_QUERY_LENGTH: usize = 3;
const SLEEP_DURATION_TO_LET_SYNC_ADVANCE: Duration = Duration::from_millis(10);
// This should be substantially bigger than SLEEP_DURATION_TO_LET_SYNC_ADVANCE.
const WAIT_PERIOD_FOR_NEW_DATA: Duration = Duration::from_millis(50);
const TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE: Duration =
    WAIT_PERIOD_FOR_NEW_DATA.saturating_add(SLEEP_DURATION_TO_LET_SYNC_ADVANCE.saturating_mul(10));

lazy_static! {
    static ref TEST_CONFIG: P2PSyncConfig = P2PSyncConfig {
        num_headers_per_query: HEADER_QUERY_LENGTH,
        num_block_state_diffs_per_query: STATE_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        stop_sync_at_block_number: None,
    };
}

#[allow(clippy::type_complexity)]
fn setup() -> (
    P2PSync,
    StorageReader,
    Receiver<Query>,
    Sender<Option<SignedBlockHeader>>,
    Sender<Option<ThinStateDiff>>,
) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (query_sender, query_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let (signed_headers_sender, signed_headers_receiver) =
        futures::channel::mpsc::channel(BUFFER_SIZE);
    let (state_diffs_sender, state_diffs_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let p2p_sync = P2PSync::new(
        *TEST_CONFIG,
        storage_reader.clone(),
        storage_writer,
        query_sender,
        ResponseReceivers {
            signed_headers_receiver: Some(signed_headers_receiver.boxed()),
            state_diffs_receiver: Some(state_diffs_receiver.boxed()),
        },
    );
    (p2p_sync, storage_reader, query_receiver, signed_headers_sender, state_diffs_sender)
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

fn create_random_state_diff(rng: &mut impl RngCore) -> ThinStateDiff {
    let contract0 = ContractAddress::from(rng.next_u64());
    let contract1 = ContractAddress::from(rng.next_u64());
    let contract2 = ContractAddress::from(rng.next_u64());
    let class_hash = ClassHash(rng.next_u64().into());
    let compiled_class_hash = CompiledClassHash(rng.next_u64().into());
    let deprecated_class_hash = ClassHash(rng.next_u64().into());
    ThinStateDiff {
        deployed_contracts: indexmap! {
            contract0 => class_hash, contract1 => class_hash, contract2 => deprecated_class_hash
        },
        storage_diffs: indexmap! {
            contract0 => indexmap! {
                1u64.into() => StarkFelt::ONE, 2u64.into() => StarkFelt::TWO
            },
            contract1 => indexmap! {
                3u64.into() => StarkFelt::TWO, 4u64.into() => StarkFelt::ONE
            },
        },
        declared_classes: indexmap! { class_hash => compiled_class_hash },
        deprecated_declared_classes: vec![deprecated_class_hash],
        nonces: indexmap! {
            contract0 => Nonce(StarkFelt::ONE), contract2 => Nonce(StarkFelt::TWO)
        },
        replaced_classes: Default::default(),
    }
}

fn split_state_diff(state_diff: ThinStateDiff) -> Vec<ThinStateDiff> {
    let mut result = Vec::new();
    if !state_diff.deployed_contracts.is_empty() {
        result.push(ThinStateDiff {
            deployed_contracts: state_diff.deployed_contracts,
            ..Default::default()
        })
    }
    if !state_diff.storage_diffs.is_empty() {
        result.push(ThinStateDiff { storage_diffs: state_diff.storage_diffs, ..Default::default() })
    }
    if !state_diff.declared_classes.is_empty() {
        result.push(ThinStateDiff {
            declared_classes: state_diff.declared_classes,
            ..Default::default()
        })
    }
    if !state_diff.deprecated_declared_classes.is_empty() {
        result.push(ThinStateDiff {
            deprecated_declared_classes: state_diff.deprecated_declared_classes,
            ..Default::default()
        })
    }
    if !state_diff.nonces.is_empty() {
        result.push(ThinStateDiff { nonces: state_diff.nonces, ..Default::default() })
    }
    if !state_diff.replaced_classes.is_empty() {
        result.push(ThinStateDiff {
            replaced_classes: state_diff.replaced_classes,
            ..Default::default()
        })
    }
    result
}

#[tokio::test]
async fn signed_headers_basic_flow() {
    const NUM_QUERIES: usize = 3;

    let (p2p_sync, storage_reader, query_receiver, mut signed_headers_sender, _state_diffs_sender) =
        setup();
    let block_hashes_and_signatures =
        create_block_hashes_and_signatures((NUM_QUERIES * HEADER_QUERY_LENGTH).try_into().unwrap());

    let mut query_receiver = query_receiver
        .filter(|query| ready(matches!(query.data_type, DataType::SignedBlockHeader)));

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        for query_index in 0..NUM_QUERIES {
            let start_block_number = query_index * HEADER_QUERY_LENGTH;
            let end_block_number = (query_index + 1) * HEADER_QUERY_LENGTH;

            // Receive query and validate it.
            let query = query_receiver.next().await.unwrap();
            assert_eq!(
                query,
                Query {
                    start_block: BlockNumber(start_block_number.try_into().unwrap()),
                    direction: Direction::Forward,
                    limit: HEADER_QUERY_LENGTH,
                    step: 1,
                    data_type: DataType::SignedBlockHeader,
                }
            );

            for (i, (block_hash, block_signature)) in block_hashes_and_signatures
                .iter()
                .enumerate()
                .take(end_block_number)
                .skip(start_block_number)
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

// TODO(shahak): Add negative tests for all state diff errors.
#[tokio::test]
async fn state_diff_basic_flow() {
    // Asserting the constants so the test can assume there will be 2 state diff queries for a
    // single header query and the second will be smaller than the first.
    const_assert!(STATE_DIFF_QUERY_LENGTH < HEADER_QUERY_LENGTH);
    const_assert!(HEADER_QUERY_LENGTH < 2 * STATE_DIFF_QUERY_LENGTH);

    let (
        p2p_sync,
        storage_reader,
        query_receiver,
        mut signed_headers_sender,
        mut state_diffs_sender,
    ) = setup();

    let block_hashes_and_signatures =
        create_block_hashes_and_signatures(HEADER_QUERY_LENGTH.try_into().unwrap());
    let mut rng = get_rng();
    let state_diffs =
        (0..HEADER_QUERY_LENGTH).map(|_| create_random_state_diff(&mut rng)).collect::<Vec<_>>();

    // We don't need to read the header query in order to know which headers to send, and we
    // already validate the query in a different test.
    let mut query_receiver =
        query_receiver.filter(|query| ready(matches!(query.data_type, DataType::StateDiff)));

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        // We wait for the state diff sync to see that there are no headers and start sleeping
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Check that before we send headers there is no state diff query.
        assert!(query_receiver.next().now_or_never().is_none());

        // Send headers for entire query.
        for (i, ((block_hash, block_signature), state_diff)) in
            block_hashes_and_signatures.iter().zip(state_diffs.iter()).enumerate()
        {
            // Send responses
            signed_headers_sender
                .send(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_number: BlockNumber(i.try_into().unwrap()),
                        block_hash: *block_hash,
                        state_diff_length: Some(state_diff.len()),
                        ..Default::default()
                    },
                    signatures: vec![*block_signature],
                }))
                .await
                .unwrap();
        }
        for (start_block_number, num_blocks) in [
            (0u64, STATE_DIFF_QUERY_LENGTH),
            (
                STATE_DIFF_QUERY_LENGTH.try_into().unwrap(),
                HEADER_QUERY_LENGTH - STATE_DIFF_QUERY_LENGTH,
            ),
        ] {
            // Get a state diff query and validate it
            let query = query_receiver.next().await.unwrap();
            assert_eq!(
                query,
                Query {
                    start_block: BlockNumber(start_block_number),
                    direction: Direction::Forward,
                    limit: num_blocks,
                    step: 1,
                    data_type: DataType::StateDiff,
                }
            );

            for block_number in
                start_block_number..(start_block_number + u64::try_from(num_blocks).unwrap())
            {
                let expected_state_diff: &ThinStateDiff =
                    &state_diffs[usize::try_from(block_number).unwrap()];
                let state_diff_parts = split_state_diff(expected_state_diff.clone());

                let block_number = BlockNumber(block_number);
                for state_diff_part in state_diff_parts {
                    // Check that before we've sent all parts the state diff wasn't written yet.
                    let txn = storage_reader.begin_ro_txn().unwrap();
                    assert_eq!(block_number, txn.get_state_marker().unwrap());

                    println!("sending state diff");
                    state_diffs_sender.send(Some(state_diff_part)).await.unwrap();
                }

                tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

                // Check state diff was written to the storage. This way we make sure that the sync
                // writes to the storage each block's state diff before receiving all query
                // responses.
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number.unchecked_next(), txn.get_state_marker().unwrap());
                let state_diff = txn.get_state_diff(block_number).unwrap().unwrap();
                assert_eq!(state_diff, *expected_state_diff);
            }
            println!("sending none");
            state_diffs_sender.send(None).await.unwrap();
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
    assert!(usize::from(NUM_ACTUAL_RESPONSES) < HEADER_QUERY_LENGTH);

    let (p2p_sync, _storage_reader, query_receiver, mut signed_headers_sender, _state_diffs_sender) =
        setup();
    let block_hashes_and_signatures = create_block_hashes_and_signatures(NUM_ACTUAL_RESPONSES);

    let mut query_receiver = query_receiver
        .filter(|query| ready(matches!(query.data_type, DataType::SignedBlockHeader)));

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
        let query = timeout(TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE, query_receiver.next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            query,
            Query {
                start_block: BlockNumber(NUM_ACTUAL_RESPONSES.into()),
                direction: Direction::Forward,
                limit: HEADER_QUERY_LENGTH,
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
