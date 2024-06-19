use std::time::Duration;

use assert_matches::assert_matches;
use futures::{FutureExt, SinkExt, StreamExt};
use indexmap::{indexmap, IndexMap};
use papyrus_common::state::create_random_state_diff;
use papyrus_protobuf::protobuf::transaction;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    Direction,
    Query,
    SignedBlockHeader,
    TransactionQuery,
};
use papyrus_storage::state::StateStorageReader;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::transaction::{Transaction, TransactionOutput};
use starknet_types_core::felt::Felt;
use static_assertions::const_assert;
use test_utils::{get_rng, GetTestInstance};

use crate::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    TestArgs,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    STATE_DIFF_QUERY_LENGTH,
};
use crate::{P2PSyncError, StateDiffQuery};

const TIMEOUT_FOR_TEST: Duration = Duration::from_secs(5);

#[tokio::test]
async fn transaction_basic_flow() {
    // Asserting the constants so the test can assume there will be 2 state diff queries for a
    // single header query and the second will be smaller than the first.
    const_assert!(STATE_DIFF_QUERY_LENGTH < HEADER_QUERY_LENGTH);
    const_assert!(HEADER_QUERY_LENGTH < 2 * STATE_DIFF_QUERY_LENGTH);

    let TestArgs {
        p2p_sync,
        storage_reader,
        mut transaction_query_receiver,
        mut headers_sender,
        mut transaction_sender,
        // The test will fail if we drop this.
        // We don't need to read the header query in order to know which headers to send, and we
        // already validate the header query in a different test.
        header_query_receiver: _header_query_receiver,
        state_diff_query_receiver: _state_diff_query_receiver,
        state_diffs_sender: _state_diffs_sender,
    } = setup();

    let block_hashes_and_signatures =
        create_block_hashes_and_signatures(HEADER_QUERY_LENGTH.try_into().unwrap());
    let mut rng = get_rng();
    let transactions =
        (0..HEADER_QUERY_LENGTH).map(|_| create_n_transactions(&mut rng)).collect::<Vec<_>>();

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        // We wait for the transaction sync to see that there are no headers and start sleeping
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Check that before we send headers there is no transaction query.
        assert!(transaction_query_receiver.next().now_or_never().is_none());

        // Send headers for entire query.
        for (i, ((block_hash, block_signature), n_transactions)) in
            block_hashes_and_signatures.iter().zip(transactions.iter()).enumerate()
        {
            // Send responses
            headers_sender
                .send((
                    Ok(DataOrFin(Some(SignedBlockHeader {
                        block_header: BlockHeader {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            block_hash: *block_hash,
                            n_transactions: Some(n_transactions.len()),
                            ..Default::default()
                        },
                        signatures: vec![*block_signature],
                    }))),
                    Box::new(|| {}),
                ))
                .await
                .unwrap();
        }
        for (start_block_number, num_blocks) in [
            (0u64, STATE_DIFF_QUERY_LENGTH),
            (STATE_DIFF_QUERY_LENGTH, HEADER_QUERY_LENGTH - STATE_DIFF_QUERY_LENGTH),
        ] {
            // Get a state diff query and validate it
            let query = transaction_query_receiver.next().await.unwrap();
            assert_eq!(
                query,
                TransactionQuery(Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit: num_blocks,
                    step: 1,
                })
            );

            for block_number in start_block_number..(start_block_number + num_blocks) {
                let expected_state_diff: &ThinStateDiff =
                    &transactions[usize::try_from(block_number).unwrap()];
                let state_diff_parts = split_state_diff(expected_state_diff.clone());

                let block_number = BlockNumber(block_number);
                for state_diff_part in state_diff_parts {
                    // Check that before we've sent all parts the state diff wasn't written yet.
                    let txn = storage_reader.begin_ro_txn().unwrap();
                    assert_eq!(block_number, txn.get_state_marker().unwrap());

                    transaction_sender
                        .send((Ok(DataOrFin(Some(state_diff_part))), Box::new(|| {})))
                        .await
                        .unwrap();
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
            state_diffs_sender.send((Ok(DataOrFin(None)), Box::new(|| {}))).await.unwrap();
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
pub fn create_n_transactions(rng: &mut ChaCha8Rng) -> (Vec<(Transaction, TransactionOutput)>) {
    let n_transactions = rng.gen_range(0..10);
    (0..n_transactions)
        .map(|_| {
            let transaction = Transaction::get_test_instance(rng);
            let transaction_output = TransactionOutput::get_test_instance(rng);
            (transaction, transaction_output)
        })
        .collect()
}

async fn validate_state_diff_fails(
    state_diff_length_in_header: usize,
    state_diff_parts: Vec<Option<ThinStateDiff>>,
    error_validator: impl Fn(P2PSyncError),
) {
    let TestArgs {
        p2p_sync,
        storage_reader,
        mut state_diff_query_receiver,
        mut headers_sender,
        mut state_diffs_sender,
        // The test will fail if we drop this.
        // We don't need to read the header query in order to know which headers to send, and we
        // already validate the header query in a different test.
        header_query_receiver: _header_query_receiver,
        ..
    } = setup();

    let (block_hash, block_signature) = *create_block_hashes_and_signatures(1).first().unwrap();

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        // Send a single header. There's no need to fill the entire query.
        headers_sender
            .send((
                Ok(DataOrFin(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_number: BlockNumber(0),
                        block_hash,
                        state_diff_length: Some(state_diff_length_in_header),
                        ..Default::default()
                    },
                    signatures: vec![block_signature],
                }))),
                Box::new(|| {}),
            ))
            .await
            .unwrap();

        // Get a state diff query and validate it
        let query = state_diff_query_receiver.next().await.unwrap();
        assert_eq!(
            query,
            StateDiffQuery(Query {
                start_block: BlockHashOrNumber::Number(BlockNumber(0)),
                direction: Direction::Forward,
                limit: 1,
                step: 1,
            })
        );

        // Send state diffs.
        for state_diff_part in state_diff_parts {
            // Check that before we've sent all parts the state diff wasn't written yet.
            let txn = storage_reader.begin_ro_txn().unwrap();
            assert_eq!(0, txn.get_state_marker().unwrap().0);

            state_diffs_sender
                .send((Ok(DataOrFin(state_diff_part)), Box::new(|| {})))
                .await
                .unwrap();
        }
        tokio::time::sleep(TIMEOUT_FOR_TEST).await;
        panic!("P2P sync did not receive error");
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            let sync_err = sync_result.unwrap_err();
            error_validator(sync_err);
        }
        _ = parse_queries_future => {}
    }
}

#[tokio::test]
async fn state_diff_empty_state_diff() {
    validate_state_diff_fails(1, vec![Some(ThinStateDiff::default())], |error| {
        assert_matches!(error, P2PSyncError::EmptyStateDiffPart)
    })
    .await;

    validate_state_diff_fails(
        1,
        vec![Some(ThinStateDiff {
            storage_diffs: indexmap! {ContractAddress::default() => IndexMap::default()},
            ..Default::default()
        })],
        |error| assert_matches!(error, P2PSyncError::EmptyStateDiffPart),
    )
    .await;
}

#[tokio::test]
async fn state_diff_stopped_in_middle() {
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                deprecated_declared_classes: vec![ClassHash::default()],
                ..Default::default()
            }),
            None,
        ],
        |error| assert_matches!(error, P2PSyncError::WrongStateDiffLength { expected_length, possible_lengths } if expected_length == 2 && possible_lengths == vec![1]),
    )
    .await;
}

#[tokio::test]
async fn state_diff_not_splitted_correctly() {
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                deprecated_declared_classes: vec![ClassHash::default()],
                ..Default::default()
            }),
            Some(ThinStateDiff {
                deprecated_declared_classes: vec![
                    ClassHash(StarkHash::ONE), ClassHash(StarkHash::TWO)
                ],
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::WrongStateDiffLength { expected_length, possible_lengths } if expected_length == 2 && possible_lengths == vec![1, 3]),
    )
    .await;
}

#[tokio::test]
async fn state_diff_conflicting() {
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                deployed_contracts: indexmap! { ContractAddress::default() => ClassHash::default() },
                ..Default::default()
            }),
            Some(ThinStateDiff {
                deployed_contracts: indexmap! { ContractAddress::default() => ClassHash::default() },
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                storage_diffs: indexmap! { ContractAddress::default() => indexmap! {
                    StorageKey::default() => Felt::default()
                }},
                ..Default::default()
            }),
            Some(ThinStateDiff {
                storage_diffs: indexmap! { ContractAddress::default() => indexmap! {
                    StorageKey::default() => Felt::default()
                }},
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                declared_classes: indexmap! {
                    ClassHash::default() => CompiledClassHash::default()
                },
                ..Default::default()
            }),
            Some(ThinStateDiff {
                declared_classes: indexmap! {
                    ClassHash::default() => CompiledClassHash::default()
                },
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                deprecated_declared_classes: vec![ClassHash::default()],
                ..Default::default()
            }),
            Some(ThinStateDiff {
                deprecated_declared_classes: vec![ClassHash::default()],
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                nonces: indexmap! { ContractAddress::default() => Nonce::default() },
                ..Default::default()
            }),
            Some(ThinStateDiff {
                nonces: indexmap! { ContractAddress::default() => Nonce::default() },
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(ThinStateDiff {
                replaced_classes: indexmap! { ContractAddress::default() => ClassHash::default() },
                ..Default::default()
            }),
            Some(ThinStateDiff {
                replaced_classes: indexmap! { ContractAddress::default() => ClassHash::default() },
                ..Default::default()
            }),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
}
