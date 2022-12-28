use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::StreamExt;
use log::{debug, error};
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{HeaderStorageReader, StateStorageReader, StorageReader, StorageWriter};
use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::hash::StarkHash;
use starknet_api::shash;
use starknet_api::state::StateDiff;

use super::central::BlocksStream;
use crate::sources::central::{MockCentralSourceTrait, StateUpdatesStream};
use crate::{CentralError, CentralSourceTrait, GenericStateSync, SyncConfig};

const SYNC_SLEEP_DURATION: Duration = Duration::new(0, 1000 * 1000 * 100); // 100ms
const DURATION_BEFORE_CHECKING_STORAGE: Duration = Duration::new(0, 1000 * 1000 * 100); // 100ms
const MAX_CHECK_STORAGE_ITERATIONS: u8 = 3;

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
    // Let the other thread opportunity to run before starting the check.
    tokio::time::sleep(DURATION_BEFORE_CHECKING_STORAGE).await;
    let interval_time = timeout.div_f32(MAX_CHECK_STORAGE_ITERATIONS.into());
    let mut interval = tokio::time::interval(interval_time);
    for i in 0..MAX_CHECK_STORAGE_ITERATIONS {
        debug!("== Checking predicate on storage ({}/{}). ==", i + 1, MAX_CHECK_STORAGE_ITERATIONS);
        match predicate(&reader) {
            CheckStoragePredicateResult::InProgress => {
                debug!("== Cechk finished, test still in progress. ==");
                interval.tick().await;
            }
            CheckStoragePredicateResult::Passed => {
                debug!("== Check passed. ==");
                return true;
            }
            CheckStoragePredicateResult::Error => {
                debug!("== Check failed. ==");
                return false;
            }
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
    let check_storage_future = check_storage(reader.clone(), Duration::from_millis(50), |reader| {
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
    const MAX_TIME_TO_SYNC_MS: u64 = 60;
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
                    block_hash: create_block_hash(block_number, false),
                    parent_hash: create_block_hash(block_number.prev().unwrap_or_default(), false),
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
                yield Ok((
                    block_number,
                    create_block_hash(block_number, false),
                    StateDiff::default(),
                    vec![])
                );
            }
        }
        .boxed();
        state_stream
    });
    mock.expect_get_block_hash()
        .returning(|bn| Ok(Some(BlockHash(shash!(format!("0x{}", bn.0).as_str())))));
    let (reader, writer) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, mock);

    // Check that the storage reached N_BLOCKS within MAX_TIME_TO_SYNC_MS.
    let check_storage_future =
        check_storage(reader, Duration::from_millis(MAX_TIME_TO_SYNC_MS), |reader| {
            let header_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            debug!("Header marker currently at {}", header_marker);
            if header_marker < BlockNumber(N_BLOCKS) {
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

fn create_block_hash(bn: BlockNumber, is_reverted_block: bool) -> BlockHash {
    if is_reverted_block {
        BlockHash(shash!(format!("0x{}10", bn.0).as_str()))
    } else {
        BlockHash(shash!(format!("0x{}", bn.0).as_str()))
    }
}
