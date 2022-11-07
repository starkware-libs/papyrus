use std::cmp::max;
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
use crate::{CentralError, GenericStateSync, StateSyncError, SyncConfig};

const SYNC_SLEEP_DURATION: Duration = Duration::new(1, 0);
const CHECK_STORAGE_INTERVAL: Duration = Duration::new(2, 0);

// Checks periodically if the storage reached a certain state defined by f.
async fn check_storage(
    reader: StorageReader,
    timeout: Duration,
    predicate: impl Fn(&StorageReader) -> bool,
) -> bool {
    let interval_time = CHECK_STORAGE_INTERVAL;
    let mut interval = tokio::time::interval(interval_time);
    let num_repeats = timeout.as_secs() / interval_time.as_secs();
    for i in 0..max(1, num_repeats) {
        debug!("Checking storage {}/{}", i, num_repeats);
        if predicate(&reader) {
            return true;
        }

        interval.tick().await;
    }
    error!("Check storage timed out.");

    false
}

// Runs sync loop with a mocked central - infinite loop unless panicking.
async fn run_sync(
    reader: StorageReader,
    writer: StorageWriter,
    central: MockCentralSourceTrait,
) -> Result<(), StateSyncError> {
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
        marker == BlockNumber(0)
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
            if header_marker != BlockNumber(N_BLOCKS) {
                return false;
            }

            let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();
            debug!("State marker currently at {}", state_marker);
            state_marker == BlockNumber(N_BLOCKS)
        });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result),
    }

    Ok(())
}
