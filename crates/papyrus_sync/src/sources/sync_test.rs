use std::cmp::max;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, error};
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{HeaderStorageReader, StateStorageReader, StorageReader, StorageWriter};
use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::hash::StarkHash;
use starknet_api::shash;
use starknet_api::state::StateDiff;
use starknet_client::ClientError;
use tokio::sync::Mutex;

use super::central::BlocksStream;
use crate::sources::central::{MockCentralSourceTrait, StateUpdatesStream};
use crate::{CentralError, CentralSourceTrait, GenericStateSync, SyncConfig};

const SYNC_SLEEP_DURATION: Duration = Duration::new(1, 0);
const CHECK_STORAGE_INTERVAL: Duration = Duration::new(2, 0);

enum CheckStoragePredicateResult {
    InProgress,
    Passed,
    Error,
}

// Checks periodically if the storage reached a certain state defined by f.
async fn check_storage(
    reader: StorageReader,
    timeout: Duration,
    predicate: impl Fn(&StorageReader) -> CheckStoragePredicateResult,
) -> bool {
    let interval_time = CHECK_STORAGE_INTERVAL;
    let mut interval = tokio::time::interval(interval_time);
    let num_repeats = timeout.as_secs() / interval_time.as_secs();
    for i in 0..max(1, num_repeats) {
        debug!("Checking storage {}/{}", i, num_repeats);
        match predicate(&reader) {
            CheckStoragePredicateResult::InProgress => {
                interval.tick().await;
            }
            CheckStoragePredicateResult::Passed => return true,
            CheckStoragePredicateResult::Error => return false,
        }
    }
    error!("Check storage timed out.");
    false
}

// Runs sync loop with a mocked central - infinite loop unless panicking.
async fn run_sync(
    reader: StorageReader,
    writer: StorageWriter,
    central: impl CentralSourceTrait + Send + Sync + 'static,
) -> Result<(), anyhow::Error> {
    let mut state_sync = GenericStateSync {
        config: SyncConfig { block_propagation_sleep_duration: SYNC_SLEEP_DURATION },
        central_source: Arc::new(central),
        reader,
        writer,
    };

    state_sync.run().await?;
    Ok(())
}

#[tokio::test]
async fn sync_empty_chain() -> Result<(), anyhow::Error> {
    let _ = simple_logger::init();

    // Mock central without any block.
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_block_marker().returning(|| Ok(BlockNumber(0)));
    let (reader, writer) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, mock);

    // Check that the header marker is 0.
    let check_storage_future = check_storage(reader.clone(), Duration::from_secs(5), |reader| {
        let marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
        if marker == BlockNumber(0) {
            return CheckStoragePredicateResult::Passed;
        }
        CheckStoragePredicateResult::Error
    });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result),
    }

    Ok(())
}

#[tokio::test]
async fn sync_happy_flow() -> Result<(), anyhow::Error> {
    const N_BLOCKS: u64 = 5;
    const MAX_TIME_TO_SYNC: u64 = 60;
    let _ = simple_logger::init();

    // Mock having N_BLOCKS chain in central.
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_block_marker().returning(|| Ok(BlockNumber(N_BLOCKS)));
    mock.expect_stream_new_blocks().returning(move |initial, up_to| {
        let blocks_stream: BlocksStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number })
                }
                let header = BlockHeader{
                    block_number,
                    block_hash: BlockHash(shash!(format!("0x{}",block_number.0).as_str())),
                    ..BlockHeader::default()
                };
                yield Ok((block_number, Block{header, body: BlockBody::default()}));
            }
        }
        .boxed();
        blocks_stream
    });
    mock.expect_stream_state_updates().returning(move |initial, up_to| {
        let state_stream: StateUpdatesStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number })
                }
                yield Ok((block_number, StateDiff::default(), vec![]));
            }
        }
        .boxed();
        state_stream
    });
    let (reader, writer) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, mock);

    // Check that the storage reached N_BLOCKS within MAX_TIME_TO_SYNC.
    let check_storage_future =
        check_storage(reader, Duration::from_secs(MAX_TIME_TO_SYNC), |reader| {
            let header_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            debug!("Header marker currently at {}", header_marker);
            if header_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::InProgress;
            }
            if header_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::Error;
            }

            let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();
            debug!("State marker currently at {}", state_marker);

            if state_marker < BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::InProgress;
            }
            if state_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::Error;
            }
            CheckStoragePredicateResult::Passed
        });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result),
    }

    Ok(())
}

// TODO: remove ignore once revert is implemented.
#[ignore = "Revert flow not supported yet."]
#[tokio::test]
async fn sync_with_revert() {
    let _ = simple_logger::init();
    let (reader, writer) = get_test_storage();

    // Shared flag for the sync and the storage check threads.
    let reverted_mutex = Arc::new(Mutex::new(false));

    // Prepare sync thread with mocked central source that will perform a revert once the
    // reverted_mutex is true.
    let mock = MockedCentralWithRevert { reverted: reverted_mutex.clone() };
    let sync_future = run_sync(reader.clone(), writer, mock);

    // Prepare functions that check that the sync worked up to N_BLOCKS_BEFORE_REVERT and then
    // reacted correctly to the revert.
    const N_BLOCKS_BEFORE_REVERT: u64 = 8;
    const MAX_TIME_TO_SYNC_BEFORE_REVERT: u64 = 60;
    const CHAIN_FORK_BLOCK_NUMBER: u64 = 5;
    const N_BLOCKS_AFTER_REVERT: u64 = 10;
    const MAX_TIME_TO_SYNC_AFTER_REVERT: u64 = 60;

    // Part 1 - check that the storage reached the point at which we will make the revert.
    let check_storage_before_revert_future = check_storage(
        reader.clone(),
        Duration::from_secs(MAX_TIME_TO_SYNC_BEFORE_REVERT),
        |reader| {
            let marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            debug!("Before revert, block marker currently at {}", marker);
            if marker.0 < N_BLOCKS_BEFORE_REVERT {
                return CheckStoragePredicateResult::InProgress;
            }
            if marker.0 == N_BLOCKS_BEFORE_REVERT {
                return CheckStoragePredicateResult::Passed;
            }
            CheckStoragePredicateResult::Error
        },
    );

    // Part 2 - signal the mocked central to simulate a revert.
    let signal_revert = async {
        debug!("Reverting.");
        let mut reverted = reverted_mutex.lock().await;
        *reverted = true;
    };

    // Part 3 - check that the storage reverted correctly.
    let check_storage_after_revert_future = check_storage(
        reader.clone(),
        Duration::from_secs(MAX_TIME_TO_SYNC_AFTER_REVERT),
        |reader| {
            let block_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            debug!("After revert, block marker currently at {}", block_marker);

            // We can't check the storage data until the marker reaches N_BLOCKS_AFTER_REVERT
            // because we can't know if the revert was already detected in the sync or not.
            if block_marker != BlockNumber(N_BLOCKS_AFTER_REVERT) {
                return CheckStoragePredicateResult::InProgress;
            }

            // We reached N_BLOCKS_AFTER_REVERT blocks, check if the state marker also reached
            // N_BLOCKS_AFTER_REVERT.
            let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();
            debug!("After revert, state marker currently at {}", state_marker);
            if state_marker != BlockNumber(N_BLOCKS_AFTER_REVERT) {
                return CheckStoragePredicateResult::InProgress;
            }

            // Both blocks and state updates are fully synced, check the data validity.
            for bn in
                BlockNumber(CHAIN_FORK_BLOCK_NUMBER).iter_up_to(BlockNumber(N_BLOCKS_AFTER_REVERT))
            {
                debug!("checking hash for block {}", bn);
                let block_header = reader.begin_ro_txn().unwrap().get_block_header(bn).unwrap();

                if block_header.is_none() {
                    error!("Block {} doesn't exist", bn);
                    return CheckStoragePredicateResult::Error;
                }
                let block_hash = block_header.unwrap().block_hash;
                let expected_block_hash = reverted_block_hash(bn);
                if block_hash != expected_block_hash {
                    error!(
                        "Wrong hash for block {}. Got {:?}, Expected {:?}.",
                        bn, block_hash, expected_block_hash
                    );
                    return CheckStoragePredicateResult::Error;
                }

                // TODO: add checks to the state diff.
            }

            CheckStoragePredicateResult::Passed
        },
    );

    // Assemble the pieces for the revert flow test.
    let check_flow = async {
        assert!(check_storage_before_revert_future.await);
        signal_revert.await;
        assert!(check_storage_after_revert_future.await);
    };

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        _ = check_flow => {},
    }

    // Mock central source that performs a revert once the reverted mutex is set to true.
    struct MockedCentralWithRevert {
        reverted: Arc<Mutex<bool>>,
    }
    impl MockedCentralWithRevert {
        fn revert_happend(&self) -> bool {
            match self.reverted.try_lock() {
                Ok(reverted) => *reverted,
                _ => false,
            }
        }
    }

    fn reverted_block_hash(bn: BlockNumber) -> BlockHash {
        BlockHash(shash!(format!("0x{}10", bn.0).as_str()))
    }

    #[async_trait]
    impl CentralSourceTrait for MockedCentralWithRevert {
        async fn get_block_marker(&self) -> Result<BlockNumber, ClientError> {
            let already_reverted = self.revert_happend();
            match already_reverted {
                false => Ok(BlockNumber(N_BLOCKS_BEFORE_REVERT)),
                true => Ok(BlockNumber(N_BLOCKS_AFTER_REVERT)),
            }
        }

        fn stream_new_blocks(
            &self,
            initial_block_number: BlockNumber,
            up_to_block_number: BlockNumber,
        ) -> BlocksStream<'_> {
            if !self.revert_happend() {
                let blocks_stream_before_revert: BlocksStream<'_> = stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_BEFORE_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i })
                        }
                        let header = BlockHeader{block_number: i, block_hash: BlockHash(shash!(format!("0x{}",i.0).as_str())), ..BlockHeader::default()};
                        yield Ok((i,Block{header, body: BlockBody::default()}));
                    }
                }.boxed();
                blocks_stream_before_revert
            } else {
                let blocks_stream_after_revert: BlocksStream<'_> = stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_AFTER_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i })
                        }
                        let header = BlockHeader{block_number: i, block_hash: reverted_block_hash(i), ..BlockHeader::default()};
                        yield Ok((i,Block{header, body: BlockBody::default()}));
                    }
                }.boxed();
                blocks_stream_after_revert
            }
        }

        fn stream_state_updates(
            &self,
            initial_block_number: BlockNumber,
            up_to_block_number: BlockNumber,
        ) -> StateUpdatesStream<'_> {
            if !self.revert_happend() {
                let state_stream_before_revert: StateUpdatesStream<'_> = stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_BEFORE_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i })
                        }
                        yield Ok((i, StateDiff::default(), vec![]));
                    }
                }
                .boxed();
                state_stream_before_revert
            } else {
                let state_stream_after_revert: StateUpdatesStream<'_> = stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_AFTER_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i })
                        }
                        yield Ok((i, StateDiff::default(), vec![]));
                    }
                }
                .boxed();
                state_stream_after_revert
            }
        }
    }
}
