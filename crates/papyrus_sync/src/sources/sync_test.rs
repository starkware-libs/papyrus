use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use async_stream::stream;
use futures::StreamExt;
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::{Block, BlockNumber, DeclaredContract, StateDiff};

use super::central::{BlocksStream, StateUpdatesStream};
use crate::sources::central::MockCentralSourceTrait;
use crate::{GenericStateSync, SyncConfig};

fn get_test_sync_config() -> SyncConfig {
    SyncConfig { block_propagation_sleep_duration: Duration::new(10, 0) }
}

#[tokio::test]
async fn sync_empty_chain() {
    let _ = simple_logger::init();
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_block_marker().returning(|| Ok(BlockNumber::new(0)));

    let (reader, writer) = get_test_storage();

    let mut state_sync = GenericStateSync {
        config: get_test_sync_config(),
        central_source: Arc::new(mock),
        reader,
        writer,
    };

    let sync_result = state_sync.run().await;
    assert_matches!(sync_result, Ok(_));
}

#[tokio::test]
async fn sync_1_block() {
    let _ = simple_logger::init();
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_block_marker().returning(|| Ok(BlockNumber::new(1)));

    let bn0 = BlockNumber::new(0);
    let blk0 = Block::default();
    let sd0 = StateDiff::default();
    let dec_contracts0 = vec![];

    mock.expect_stream_new_blocks().times(1).returning(move |_initial, _up_to| {
        let blocks: Vec<(BlockNumber, Block)> = vec![(bn0, blk0.clone())];
        let blocks_stream: BlocksStream = stream! {for i in blocks {
            yield Ok(i);
        }
        }
        .boxed();
        blocks_stream
    });

    mock.expect_stream_state_updates().times(1).returning(move |_initial, _up_to| {
        let state_updates: Vec<(BlockNumber, StateDiff, Vec<DeclaredContract>)> = vec![(bn0, sd0.clone(), dec_contracts0.clone())];
        let state_updates_stream: StateUpdatesStream = stream! {for i in state_updates {
            yield Ok(i);
        }
        }
        .boxed();
        state_updates_stream
    });

    let (reader, writer) = get_test_storage();

    let mut state_sync = GenericStateSync {
        config: get_test_sync_config(),
        central_source: Arc::new(mock),
        reader,
        writer,
    };

    let sync_result = state_sync.run().await;
    assert_matches!(sync_result, Ok(_));
}
