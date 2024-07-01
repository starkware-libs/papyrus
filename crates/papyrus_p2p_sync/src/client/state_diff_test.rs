use std::time::Duration;

use assert_matches::assert_matches;
use futures::{FutureExt, SinkExt, StreamExt};
use indexmap::indexmap;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
};
use papyrus_storage::state::StateStorageReader;
use rand::RngCore;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;
use static_assertions::const_assert;
use test_utils::{get_rng, GetTestInstance};

use super::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    TestArgs,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    STATE_DIFF_QUERY_LENGTH,
};
use super::{P2PSyncError, StateDiffQuery};

const TIMEOUT_FOR_TEST: Duration = Duration::from_secs(5);

#[tokio::test]
async fn state_diff_basic_flow() {
    // Asserting the constants so the test can assume there will be 2 state diff queries for a
    // single header query and the second will be smaller than the first.
    const_assert!(STATE_DIFF_QUERY_LENGTH < HEADER_QUERY_LENGTH);
    const_assert!(HEADER_QUERY_LENGTH < 2 * STATE_DIFF_QUERY_LENGTH);

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

    let block_hashes_and_signatures =
        create_block_hashes_and_signatures(HEADER_QUERY_LENGTH.try_into().unwrap());
    let mut rng = get_rng();
    // TODO(eitan): Add a 3rd constant for NUM_CHUNKS_PER_BLOCK so that ThinStateDiff is made from
    // multiple StateDiffChunks
    let state_diffs = (0..HEADER_QUERY_LENGTH)
        .map(|_| create_random_state_diff_chunk(&mut rng))
        .collect::<Vec<_>>();

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        // We wait for the state diff sync to see that there are no headers and start sleeping
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Check that before we send headers there is no state diff query.
        assert!(state_diff_query_receiver.next().now_or_never().is_none());

        // Send headers for entire query.
        for (i, ((block_hash, block_signature), state_diff)) in
            block_hashes_and_signatures.iter().zip(state_diffs.iter()).enumerate()
        {
            // Send responses
            headers_sender
                .send((
                    Ok(DataOrFin(Some(SignedBlockHeader {
                        block_header: BlockHeader {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            block_hash: *block_hash,
                            state_diff_length: Some(state_diff.len()),
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
            let query = state_diff_query_receiver.next().await.unwrap();
            assert_eq!(
                query,
                StateDiffQuery(Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit: num_blocks,
                    step: 1,
                })
            );

            for block_number in start_block_number..(start_block_number + num_blocks) {
                let state_diff_chunk = state_diffs[usize::try_from(block_number).unwrap()].clone();

                let block_number = BlockNumber(block_number);

                // Check that before we've sent all parts the state diff wasn't written yet.
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number, txn.get_state_marker().unwrap());

                state_diffs_sender
                    .send((Ok(DataOrFin(Some(state_diff_chunk.clone()))), Box::new(|| {})))
                    .await
                    .unwrap();

                tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

                // Check state diff was written to the storage. This way we make sure that the sync
                // writes to the storage each block's state diff before receiving all query
                // responses.
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number.unchecked_next(), txn.get_state_marker().unwrap());

                let state_diff = txn.get_state_diff(block_number).unwrap().unwrap();
                let expected_state_diff = match state_diff_chunk {
                    StateDiffChunk::ContractDiff(contract_diff) => {
                        let mut deployed_contracts = indexmap! {};
                        if let Some(class_hash) = contract_diff.class_hash {
                            deployed_contracts.insert(contract_diff.contract_address, class_hash);
                        };
                        let mut nonces = indexmap! {};
                        if let Some(nonce) = contract_diff.nonce {
                            nonces.insert(contract_diff.contract_address, nonce);
                        }
                        ThinStateDiff {
                            deployed_contracts,
                            nonces,
                            storage_diffs: indexmap! {
                                contract_diff.contract_address => contract_diff.storage_diffs
                            },
                            ..Default::default()
                        }
                    }
                    StateDiffChunk::DeclaredClass(declared_class) => ThinStateDiff {
                        declared_classes: indexmap! {
                            declared_class.class_hash => declared_class.compiled_class_hash
                        },
                        ..Default::default()
                    },
                    StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
                        ThinStateDiff {
                            deprecated_declared_classes: vec![deprecated_declared_class.class_hash],
                            ..Default::default()
                        }
                    }
                };
                assert_eq!(state_diff, expected_state_diff);
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

#[tokio::test]
async fn state_diff_empty_state_diff() {
    validate_state_diff_fails(1, vec![Some(StateDiffChunk::default())], |error| {
        assert_matches!(error, P2PSyncError::EmptyStateDiffPart)
    })
    .await;
}

#[tokio::test]
async fn state_diff_stopped_in_middle() {
    validate_state_diff_fails(
        2,
        vec![
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass::default())),
            None,
        ],
        |error| assert_matches!(error, P2PSyncError::WrongStateDiffLength { expected_length, possible_lengths } if expected_length == 2 && possible_lengths == vec![1]),
    )
    .await;
}

#[tokio::test]
async fn state_diff_not_split_correctly() {
    validate_state_diff_fails(
        2,
        vec![
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass::default())),
            Some(StateDiffChunk::ContractDiff(ContractDiff{
                contract_address: ContractAddress::default(),
                class_hash: Some(ClassHash::default()),
                nonce: Some(Nonce::default()),
                ..Default::default()
            }),)
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
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                class_hash: Some(ClassHash::default()),
                ..Default::default()
            })),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                class_hash: Some(ClassHash::default()),
                ..Default::default()
            })),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                storage_diffs: indexmap! { StorageKey::default() => Felt::default() },
                ..Default::default()
            })),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                storage_diffs: indexmap! { StorageKey::default() => Felt::default() },
                ..Default::default()
            })),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(StateDiffChunk::DeclaredClass(DeclaredClass {
                class_hash: ClassHash::default(),
                compiled_class_hash: CompiledClassHash::default(),
            })),
            Some(StateDiffChunk::DeclaredClass(DeclaredClass {
                class_hash: ClassHash::default(),
                compiled_class_hash: CompiledClassHash::default(),
            })),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass {
                class_hash: ClassHash::default(),
            })),
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass {
                class_hash: ClassHash::default(),
            })),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
    validate_state_diff_fails(
        2,
        vec![
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                nonce: Some(Nonce::default()),
                ..Default::default()
            })),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                nonce: Some(Nonce::default()),
                ..Default::default()
            })),
        ],
        |error| assert_matches!(error, P2PSyncError::ConflictingStateDiffParts),
    )
    .await;
}

async fn validate_state_diff_fails(
    state_diff_length_in_header: usize,
    state_diff_chunks: Vec<Option<StateDiffChunk>>,
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
        for state_diff_chunk in state_diff_chunks {
            // Check that before we've sent all parts the state diff wasn't written yet.
            let txn = storage_reader.begin_ro_txn().unwrap();
            assert_eq!(0, txn.get_state_marker().unwrap().0);

            state_diffs_sender
                .send((Ok(DataOrFin(state_diff_chunk)), Box::new(|| {})))
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

fn create_random_state_diff_chunk(rng: &mut ChaCha8Rng) -> StateDiffChunk {
    let mut state_diff_chunk = StateDiffChunk::get_test_instance(rng);
    let contract_address = ContractAddress::from(rng.next_u64());
    let class_hash = ClassHash(rng.next_u64().into());
    match &mut state_diff_chunk {
        StateDiffChunk::ContractDiff(contract_diff) => {
            contract_diff.contract_address = contract_address;
            contract_diff.class_hash = Some(class_hash);
        }
        StateDiffChunk::DeclaredClass(declared_class) => {
            declared_class.class_hash = class_hash;
            declared_class.compiled_class_hash = CompiledClassHash(rng.next_u64().into());
        }
        StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
            deprecated_declared_class.class_hash = class_hash;
        }
    }
    state_diff_chunk
}
