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
