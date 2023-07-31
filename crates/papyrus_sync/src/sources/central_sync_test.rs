use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use indexmap::IndexMap;
use papyrus_common::SyncingState;
use papyrus_storage::base_layer::{BaseLayerStorageReader, BaseLayerStorageWriter};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter, StarknetVersion};
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
use starknet_api::state::StateDiff;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error};

use super::BaseLayerSourceTrait;
use crate::sources::base_layer::MockBaseLayerSourceTrait;
use crate::sources::central::{
    BlocksStream, CompiledClassesStream, MockCentralSourceTrait, StateUpdatesStream,
};
use crate::{
    stream_new_base_layer_block, CentralError, CentralSourceTrait, GenericStateSync,
    StateSyncError, StateSyncResult, SyncConfig, SyncEvent,
};

const SYNC_SLEEP_DURATION: Duration = Duration::from_millis(100); // 100ms
const BASE_LAYER_SLEEP_DURATION: Duration = Duration::from_millis(10); // 10ms
const DURATION_BEFORE_CHECKING_STORAGE: Duration = SYNC_SLEEP_DURATION.saturating_mul(2); // 200ms twice the sleep duration of the sync loop.
const MAX_CHECK_STORAGE_ITERATIONS: u8 = 3;
const STREAM_SIZE: u32 = 1000;
const STARKNET_VERSION: &str = "starknet_version";

// TODO(dvir): consider adding a test for mismatch between the base layer and l2.

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
                debug!("== Check finished, test still in progress. ==");
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
    base_layer: impl BaseLayerSourceTrait + Send + Sync,
) -> StateSyncResult {
    let mut state_sync = GenericStateSync {
        config: SyncConfig {
            block_propagation_sleep_duration: SYNC_SLEEP_DURATION,
            base_layer_propagation_sleep_duration: BASE_LAYER_SLEEP_DURATION,
            recoverable_error_sleep_duration: SYNC_SLEEP_DURATION,
            blocks_max_stream_size: STREAM_SIZE,
            state_updates_max_stream_size: STREAM_SIZE,
        },
        shared_syncing_state: Arc::new(RwLock::new(SyncingState::default())),
        central_source: Arc::new(central),
        base_layer_source: Arc::new(base_layer),
        reader,
        writer,
    };

    state_sync.run().await?;
    Ok(())
}

#[tokio::test]
async fn sync_empty_chain() {
    let _ = simple_logger::init_with_env();

    // Mock central without any block.
    let mut central_mock = MockCentralSourceTrait::new();
    central_mock.expect_get_block_marker().returning(|| Ok(BlockNumber(0)));

    // Mock base_layer without any block.
    let mut base_layer_mock = MockBaseLayerSourceTrait::new();
    base_layer_mock.expect_latest_proved_block().returning(|| Ok(None));

    let ((reader, writer), _temp_dir) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, central_mock, base_layer_mock);

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
}

#[tokio::test]
async fn sync_happy_flow() {
    const N_BLOCKS: u64 = 5;
    // FIXME: (Omri) analyze and set a lower value.
    const MAX_TIME_TO_SYNC_MS: u64 = 1000;
    let _ = simple_logger::init_with_env();

    // Mock having N_BLOCKS chain in central.
    let mut central_mock = MockCentralSourceTrait::new();
    central_mock.expect_get_block_marker().returning(|| Ok(BlockNumber(N_BLOCKS)));
    central_mock.expect_stream_new_blocks().returning(move |initial, up_to| {
        let blocks_stream: BlocksStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number });
                }
                let header = BlockHeader {
                    block_number,
                    block_hash: create_block_hash(block_number, false),
                    parent_hash: create_block_hash(block_number.prev().unwrap_or_default(), false),
                    ..BlockHeader::default()
                };
                yield Ok((block_number, Block { header, body: BlockBody::default() }, StarknetVersion(STARKNET_VERSION.to_string())));
            }
        }
        .boxed();
        blocks_stream
    });
    central_mock.expect_stream_state_updates().returning(move |initial, up_to| {
        let state_stream: StateUpdatesStream<'_> = stream! {
            for block_number in initial.iter_up_to(up_to) {
                if block_number.0 >= N_BLOCKS {
                    yield Err(CentralError::BlockNotFound { block_number })
                }
                yield Ok((
                    block_number,
                    create_block_hash(block_number, false),
                    StateDiff::default(),
                    IndexMap::new(),
                ));
            }
        }
        .boxed();
        state_stream
    });
    central_mock.expect_get_block_hash().returning(|bn| Ok(Some(create_block_hash(bn, false))));

    // TODO(dvir): find a better way to do this.
    let mut base_layer_mock = MockBaseLayerSourceTrait::new();
    let mut base_layer_call_counter = 0;
    base_layer_mock.expect_latest_proved_block().returning(move || {
        base_layer_call_counter += 1;
        Ok(match base_layer_call_counter {
            1 => None,
            2 => Some((
                BlockNumber(N_BLOCKS - 2),
                create_block_hash(BlockNumber(N_BLOCKS - 2), false),
            )),
            _ => Some((
                BlockNumber(N_BLOCKS - 1),
                create_block_hash(BlockNumber(N_BLOCKS - 1), false),
            )),
        })
    });

    let ((reader, writer), _temp_dir) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, central_mock, base_layer_mock);

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

            let base_layer_marker =
                reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
            debug!("Base layer marker currently at {base_layer_marker}");
            if base_layer_marker < BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::InProgress;
            }
            if base_layer_marker > BlockNumber(N_BLOCKS) {
                return CheckStoragePredicateResult::Error;
            }

            CheckStoragePredicateResult::Passed
        });

    tokio::select! {
        sync_result = sync_future => sync_result.unwrap(),
        storage_check_result = check_storage_future => assert!(storage_check_result),
    }
}

#[tokio::test]
async fn sync_with_revert() {
    let _ = simple_logger::init_with_env();
    let ((reader, writer), _temp_dir) = get_test_storage();

    // Once the sync reaches N_BLOCKS_BEFORE_REVERT, the check_storage thread will set this flag to
    // true to mark the central to simulate a revert, and for the check_storage to start checking
    // for the new blocks after the revert.
    let reverted_mutex = Arc::new(Mutex::new(false));

    // Prepare sync thread with mocked central source that will perform a revert once the
    // reverted_mutex is true.
    let mock = MockedCentralWithRevert { reverted: reverted_mutex.clone() };
    let mut base_layer_mock = MockBaseLayerSourceTrait::new();
    base_layer_mock.expect_latest_proved_block().returning(|| Ok(None));
    let sync_future = run_sync(reader.clone(), writer, mock, base_layer_mock);

    // Prepare functions that check that the sync worked up to N_BLOCKS_BEFORE_REVERT and then
    // reacted correctly to the revert.
    const N_BLOCKS_BEFORE_REVERT: u64 = 8;
    const MAX_TIME_TO_SYNC_BEFORE_REVERT_MS: u64 = 100;
    const CHAIN_FORK_BLOCK_NUMBER: u64 = 5;
    const N_BLOCKS_AFTER_REVERT: u64 = 10;
    // FIXME: (Omri) analyze and set a lower value.
    const MAX_TIME_TO_SYNC_AFTER_REVERT_MS: u64 = 900;

    // Part 1 - check that the storage reached the point at which we will make the revert.
    let check_storage_before_revert_future = check_storage(
        reader.clone(),
        Duration::from_millis(MAX_TIME_TO_SYNC_BEFORE_REVERT_MS),
        |reader| {
            let marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            debug!("Before revert, block marker currently at {}", marker);
            match marker {
                BlockNumber(bn) if bn < N_BLOCKS_BEFORE_REVERT => {
                    CheckStoragePredicateResult::InProgress
                }
                BlockNumber(bn) if bn == N_BLOCKS_BEFORE_REVERT => {
                    CheckStoragePredicateResult::Passed
                }
                _ => CheckStoragePredicateResult::Error,
            }
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
        Duration::from_millis(MAX_TIME_TO_SYNC_AFTER_REVERT_MS),
        |reader| {
            let block_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
            let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();
            debug!(
                "Block marker currently at {}, state marker currently at {}.",
                block_marker, state_marker
            );

            // We can't check the storage data until the marker reaches N_BLOCKS_AFTER_REVERT
            // because we can't know if the revert was already detected in the sync or not.
            // Check both markers.
            match (block_marker, state_marker) {
                (BlockNumber(bm), BlockNumber(sm))
                    if bm > N_BLOCKS_AFTER_REVERT || sm > N_BLOCKS_AFTER_REVERT =>
                {
                    CheckStoragePredicateResult::Error
                }

                (BlockNumber(bm), BlockNumber(sm))
                    if bm < N_BLOCKS_AFTER_REVERT || sm < N_BLOCKS_AFTER_REVERT =>
                {
                    CheckStoragePredicateResult::InProgress
                }
                (BlockNumber(bm), BlockNumber(sm))
                    if bm == N_BLOCKS_AFTER_REVERT && sm == N_BLOCKS_AFTER_REVERT =>
                {
                    // Both blocks and state updates are fully synced, check the data validity.
                    for bn in BlockNumber(CHAIN_FORK_BLOCK_NUMBER)
                        .iter_up_to(BlockNumber(N_BLOCKS_AFTER_REVERT))
                    {
                        debug!("checking hash for block {}", bn);
                        let block_header =
                            reader.begin_ro_txn().unwrap().get_block_header(bn).unwrap();

                        if block_header.is_none() {
                            error!("Block {} doesn't exist", bn);
                            return CheckStoragePredicateResult::Error;
                        }
                        let block_hash = block_header.unwrap().block_hash;
                        let expected_block_hash = create_block_hash(bn, true);
                        if block_hash != expected_block_hash {
                            error!(
                                "Wrong hash for block {}. Got {}, Expected {}.",
                                bn, block_hash, expected_block_hash
                            );
                            return CheckStoragePredicateResult::Error;
                        }

                        // TODO: add checks to the state diff.
                    }

                    CheckStoragePredicateResult::Passed
                }
                _ => unreachable!("Should never happen."),
            }
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

    #[async_trait]
    impl CentralSourceTrait for MockedCentralWithRevert {
        async fn get_block_marker(&self) -> Result<BlockNumber, CentralError> {
            let already_reverted = self.revert_happend();
            match already_reverted {
                false => Ok(BlockNumber(N_BLOCKS_BEFORE_REVERT)),
                true => Ok(BlockNumber(N_BLOCKS_AFTER_REVERT)),
            }
        }

        async fn get_block_hash(
            &self,
            block_number: BlockNumber,
        ) -> Result<Option<BlockHash>, CentralError> {
            match (self.revert_happend(), block_number) {
                (false, BlockNumber(bn)) if bn >= N_BLOCKS_BEFORE_REVERT => Ok(None),
                (false, BlockNumber(bn)) if bn < N_BLOCKS_BEFORE_REVERT => {
                    Ok(Some(create_block_hash(block_number, false)))
                }
                (true, BlockNumber(bn)) if bn >= N_BLOCKS_AFTER_REVERT => Ok(None),
                (true, BlockNumber(bn)) if bn >= CHAIN_FORK_BLOCK_NUMBER => {
                    Ok(Some(create_block_hash(block_number, true)))
                }
                (true, BlockNumber(bn)) if bn < CHAIN_FORK_BLOCK_NUMBER => {
                    Ok(Some(create_block_hash(block_number, false)))
                }
                _ => unreachable!(
                    "get_block_hash when Revert happend: {}, bn: {}",
                    self.revert_happend(),
                    block_number
                ),
            }
        }

        fn stream_new_blocks(
            &self,
            initial_block_number: BlockNumber,
            up_to_block_number: BlockNumber,
        ) -> BlocksStream<'_> {
            match self.revert_happend() {
                false => stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_BEFORE_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i });
                        }
                        let header = BlockHeader{
                            block_number: i,
                            block_hash: create_block_hash(i, false),
                            parent_hash: create_block_hash(i.prev().unwrap_or_default(), false),
                            ..BlockHeader::default()};
                        yield Ok((i,Block{header, body: BlockBody::default()}, StarknetVersion(STARKNET_VERSION.to_string())));
                    }
                }
                .boxed(),
                true => stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_AFTER_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i });
                        }
                        let header = BlockHeader{
                            block_number: i,
                            block_hash: create_block_hash(i, i.0 >= CHAIN_FORK_BLOCK_NUMBER),
                            parent_hash: create_block_hash(i.prev().unwrap_or_default(), i.0 > CHAIN_FORK_BLOCK_NUMBER),
                            ..BlockHeader::default()};
                        yield Ok((i, Block{header, body: BlockBody::default()},  StarknetVersion(STARKNET_VERSION.to_string())));
                    }
                }
                .boxed(),
            }
        }

        fn stream_state_updates(
            &self,
            initial_block_number: BlockNumber,
            up_to_block_number: BlockNumber,
        ) -> StateUpdatesStream<'_> {
            match self.revert_happend() {
                false => stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_BEFORE_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i });
                        }
                        yield Ok((i, create_block_hash(i, false), StateDiff::default(), IndexMap::new()));
                    }
                }
                .boxed(),
                true => stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_AFTER_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i });
                        }
                        let is_reverted_state_diff = i.0 >= CHAIN_FORK_BLOCK_NUMBER;
                        yield Ok((
                            i,
                            create_block_hash(i, is_reverted_state_diff),
                            StateDiff::default(),
                            IndexMap::new(),
                        ));
                    }
                }
                .boxed(),
            }
        }

        fn stream_compiled_classes(
            &self,
            _initial_block_number: BlockNumber,
            _up_to_block_number: BlockNumber,
        ) -> CompiledClassesStream<'_> {
            // An empty stream.
            let res: CompiledClassesStream<'_> = stream! {
                for i in [] {
                    yield i;
                }
            }
            .boxed();
            res
        }
    }
}

#[tokio::test]
async fn test_unrecoverable_sync_error_flow() {
    let _ = simple_logger::init_with_env();

    const BLOCK_NUMBER: BlockNumber = BlockNumber(1);
    const WRONG_BLOCK_NUMBER: BlockNumber = BlockNumber(2);

    // Mock central with one block but return wrong header.
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_block_marker().returning(|| Ok(BLOCK_NUMBER));
    mock.expect_stream_new_blocks().returning(move |_, _| {
        let blocks_stream: BlocksStream<'_> = stream! {
            let header = BlockHeader {
                    block_number: BLOCK_NUMBER,
                    block_hash: create_block_hash(BLOCK_NUMBER, false),
                    parent_hash: create_block_hash(BLOCK_NUMBER.prev().unwrap_or_default(), false),
                    ..BlockHeader::default()
                };
            yield Ok((
                BLOCK_NUMBER,
                Block { header, body: BlockBody::default()},
                StarknetVersion(STARKNET_VERSION.to_string()),
            ));
        }
        .boxed();
        blocks_stream
    });
    mock.expect_stream_state_updates().returning(move |_, _| {
        let state_stream: StateUpdatesStream<'_> = stream! {
            yield Ok((
                BLOCK_NUMBER,
                create_block_hash(BLOCK_NUMBER, false),
                StateDiff::default(),
                IndexMap::new(),
            ));
        }
        .boxed();
        state_stream
    });
    // make get_block_hash return a hash for the wrong block number
    mock.expect_get_block_hash()
        .returning(|_| Ok(Some(create_block_hash(WRONG_BLOCK_NUMBER, false))));

    let ((reader, writer), _temp_dir) = get_test_storage();
    let sync_future = run_sync(reader.clone(), writer, mock, MockBaseLayerSourceTrait::new());
    let sync_res = tokio::join! {sync_future};
    assert!(sync_res.0.is_err());
    // expect sync to raise the unrecoverable error it gets. In this case a DB Inconsistency error.
    assert_matches!(
        sync_res.0.unwrap_err(),
        StateSyncError::StorageError(StorageError::DBInconsistency { msg: _ })
    );
}

fn create_block_hash(bn: BlockNumber, is_reverted_block: bool) -> BlockHash {
    if is_reverted_block {
        BlockHash(stark_felt!(format!("0x{}10", bn.0).as_str()))
    } else {
        BlockHash(stark_felt!(format!("0x{}", bn.0).as_str()))
    }
}

// Adds to the storage 'headers_num' headers.
fn add_headers(headers_num: u64, writer: &mut StorageWriter) {
    for i in 0..headers_num {
        let header = BlockHeader {
            block_number: BlockNumber(i),
            block_hash: BlockHash(i.into()),
            ..BlockHeader::default()
        };
        writer
            .begin_rw_txn()
            .unwrap()
            .append_header(BlockNumber(i), &header)
            .unwrap()
            .commit()
            .unwrap();
    }
}

#[tokio::test]
async fn stream_new_base_layer_block_test_header_marker() {
    let (reader, mut writer) = get_test_storage().0;

    // Header marker points to to block number 5.
    add_headers(5, &mut writer);

    // TODO(dvir): find a better way to do it.
    // Base layer after the header marker, skip 5 and 10 and return only 1 and 4.
    let block_numbers = vec![5, 1, 10, 4];
    let mut iter = block_numbers.into_iter().map(|bn| (BlockNumber(bn), BlockHash::default()));
    let mut mock = MockBaseLayerSourceTrait::new();
    mock.expect_latest_proved_block().times(4).returning(move || Ok(iter.next()));
    let mut stream =
        stream_new_base_layer_block(reader, Arc::new(mock), Duration::from_millis(0)).boxed();

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(1), .. });

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(4), .. });
}

#[tokio::test]
async fn stream_new_base_layer_block_test_base_layer_marker() {
    let (reader, mut writer) = get_test_storage().0;

    // Header marker points to to block number 12.
    add_headers(12, &mut writer);

    // Base layer marker points to to block number 5.
    writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&BlockNumber(5))
        .unwrap()
        .commit()
        .unwrap();

    // If base layer marker is not behind the real base layer then no blocks should be returned.
    let block_numbers = vec![5, 1, 10, 4];
    let mut iter = block_numbers.into_iter().map(|bn| (BlockNumber(bn), BlockHash::default()));
    let mut mock = MockBaseLayerSourceTrait::new();
    mock.expect_latest_proved_block().times(3).returning(move || Ok(iter.next()));
    let mut stream =
        stream_new_base_layer_block(reader, Arc::new(mock), Duration::from_millis(0)).boxed();

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(5), .. });

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(10), .. });
}

#[tokio::test]
async fn stream_new_base_layer_block_no_blocks_on_base_layer() {
    let (reader, mut writer) = get_test_storage().0;

    // Header marker points to to block number 5.
    add_headers(5, &mut writer);

    // In the first polling of the base layer no blocks were found, in the second polling a block
    // was found.
    let mut values = vec![None, Some((BlockNumber(1), BlockHash::default()))].into_iter();
    let mut mock = MockBaseLayerSourceTrait::new();
    mock.expect_latest_proved_block().times(2).returning(move || Ok(values.next().unwrap()));

    let mut stream =
        stream_new_base_layer_block(reader, Arc::new(mock), Duration::from_millis(0)).boxed();

    let event = stream.next().await.unwrap().unwrap();
    assert_matches!(event, SyncEvent::NewBaseLayerBlock { block_number: BlockNumber(1), .. });
}

#[test]
fn store_base_layer_block_test() {
    let (reader, mut writer) = get_test_storage().0;

    let header_hash = BlockHash(stark_felt!("0x0"));
    let header = BlockHeader {
        block_number: BlockNumber(0),
        block_hash: header_hash,
        ..BlockHeader::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &header)
        .unwrap()
        .commit()
        .unwrap();

    let mut gen_state_sync = GenericStateSync {
        config: SyncConfig {
            block_propagation_sleep_duration: SYNC_SLEEP_DURATION,
            base_layer_propagation_sleep_duration: BASE_LAYER_SLEEP_DURATION,
            recoverable_error_sleep_duration: SYNC_SLEEP_DURATION,
            blocks_max_stream_size: STREAM_SIZE,
            state_updates_max_stream_size: STREAM_SIZE,
        },
        central_source: Arc::new(MockCentralSourceTrait::new()),
        base_layer_source: Arc::new(MockBaseLayerSourceTrait::new()),
        reader,
        writer,
    };

    // Trying to store a block without a header in the storage.
    let res = gen_state_sync.store_base_layer_block(BlockNumber(1), BlockHash::default());
    assert_matches!(res, Err(StateSyncError::BaseLayerBlockWithoutMatchingHeader { .. }));

    // Trying to store a block with mismatching header.
    let res =
        gen_state_sync.store_base_layer_block(BlockNumber(0), BlockHash(stark_felt!("0x666")));
    assert_matches!(res, Err(StateSyncError::BaseLayerHashMismatch { .. }));

    // Happy flow.
    let res = gen_state_sync.store_base_layer_block(BlockNumber(0), header_hash);
    assert!(res.is_ok());
    let base_layer_marker =
        gen_state_sync.reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
    assert_eq!(base_layer_marker, BlockNumber(1));
}
