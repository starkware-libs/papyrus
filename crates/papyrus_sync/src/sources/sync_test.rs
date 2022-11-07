use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::StreamExt;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{HeaderStorageReader, StorageReader, StorageWriter};
use starknet_api::{
    shash, Block, BlockBody, BlockHash, BlockHeader, BlockNumber, StarkHash, StateDiff,
};

use super::central::BlocksStream;
use crate::sources::central::{MockCentralSourceTrait, StateUpdatesStream};
use crate::{CentralError, GenericStateSync, SyncConfig};

const SYNC_SLEEP_DURATION: u8 = 1;
const CHECK_STORAGE_INTERVAL: u8 = 2;

/// Checks periodically if the storage reached a certain state defined by f.
async fn check_storage(
    reader: StorageReader,
    timeout: Duration,
    f: impl Fn(&StorageReader) -> anyhow::Result<bool>,
) -> anyhow::Result<bool> {
    let interval_time = Duration::from_secs(CHECK_STORAGE_INTERVAL.into());
    let mut interval = tokio::time::interval(interval_time);
    let num_repeats = timeout.as_secs() / interval_time.as_secs();
    for i in 0..num_repeats {
        println!("Checking storage {}/{}", i, num_repeats);
        if f(&reader)? {
            return Ok(true);
        }

        interval.tick().await;
    }
    println!("Check storage timed out.");

    Ok(false)
}

/// Runs sync loop with a mocked central - infinite loop unless panicking.
async fn run_sync(
    reader: StorageReader,
    writer: StorageWriter,
    central: MockCentralSourceTrait,
) -> Result<(), anyhow::Error> {
    let mut state_sync = GenericStateSync {
        config: SyncConfig {
            block_propagation_sleep_duration: Duration::new(SYNC_SLEEP_DURATION.into(), 0),
        },
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
    mock.expect_get_block_marker().returning(|| Ok(BlockNumber::new(0)));
    let (reader, writer) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, mock);

    // Check that the header marker is 0.
    let check_storage_future = check_storage(reader.clone(), Duration::from_secs(5), |reader| {
        let marker = reader.begin_ro_txn()?.get_header_marker()?;
        Ok(marker == BlockNumber::new(0))
    });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result?),
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
    mock.expect_get_block_marker().returning(|| Ok(BlockNumber::new(N_BLOCKS.into())));
    mock.expect_stream_new_blocks().returning(move |initial, up_to| {
        let blocks_stream: BlocksStream = stream! {
            for i in initial.iter_up_to(up_to) {
                if i.number() >= &N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number: i })
                }
                let header = BlockHeader{block_number: i, block_hash: BlockHash::new(shash!(format!("0x{}",i.number()).as_str())), ..BlockHeader::default()};
                yield Ok((i,Block{header, body: BlockBody::default()}));
            }
        }
        .boxed();
        blocks_stream
    });
    mock.expect_stream_state_updates().returning(move |initial, up_to| {
        let state_stream: StateUpdatesStream = stream! {
            for i in initial.iter_up_to(up_to) {
                if i.number() >= &N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number: i })
                }
                yield Ok((i, StateDiff::default(), vec![]));
            }
        }
        .boxed();
        state_stream
    });
    let (reader, writer) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, mock);

    // Check that the storage reached N_BLOCKS within MAX_TIME_TO_SYNC.
    let check_storage_future = check_storage(reader, Duration::from_secs(60), |reader| {
        let marker = reader.begin_ro_txn()?.get_header_marker()?;
        println!("Block marker currently at {}", marker);
        Ok(marker == BlockNumber::new(N_BLOCKS))
    });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result?),
    }

    Ok(())
}
