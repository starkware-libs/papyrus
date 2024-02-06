use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use async_stream::stream;
use async_trait::async_trait;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use futures::StreamExt;
use indexmap::IndexMap;
use papyrus_common::pending_classes::{ApiContractClass, PendingClasses};
use papyrus_common::BlockHashAndNumber;
use papyrus_storage::base_layer::BaseLayerStorageReader;
use papyrus_storage::header::{HeaderStorageReader, StarknetVersion};
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::ClassHash;
use starknet_api::state::StateDiff;
use starknet_client::reader::PendingData;
use starknet_types_core::felt::Felt;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error};

use super::pending::MockPendingSourceTrait;
use crate::sources::base_layer::{BaseLayerSourceTrait, MockBaseLayerSourceTrait};
use crate::sources::central::{
    BlocksStream,
    CompiledClassesStream,
    MockCentralSourceTrait,
    StateUpdatesStream,
};
use crate::{
    CentralError,
    CentralSourceTrait,
    GenericStateSync,
    StateSyncError,
    StateSyncResult,
    SyncConfig,
};

const SYNC_SLEEP_DURATION: Duration = Duration::from_millis(100); // 100ms
const BASE_LAYER_SLEEP_DURATION: Duration = Duration::from_millis(10); // 10ms
const DURATION_BEFORE_CHECKING_STORAGE: Duration = SYNC_SLEEP_DURATION.saturating_mul(2); // 200ms twice the sleep duration of the sync loop.
const MAX_CHECK_STORAGE_ITERATIONS: u8 = 3;
const STREAM_SIZE: u32 = 1000;
const STARKNET_VERSION: &str = "starknet_version";

// TODO(dvir): separate this file to flow tests and unit tests.
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
    // Mock to the pending source that always returns the default pending data.
    let mut pending_source = MockPendingSourceTrait::new();
    pending_source.expect_get_pending_data().returning(|| Ok(PendingData::default()));

    let mut state_sync = GenericStateSync {
        config: SyncConfig {
            block_propagation_sleep_duration: SYNC_SLEEP_DURATION,
            base_layer_propagation_sleep_duration: BASE_LAYER_SLEEP_DURATION,
            recoverable_error_sleep_duration: SYNC_SLEEP_DURATION,
            blocks_max_stream_size: STREAM_SIZE,
            state_updates_max_stream_size: STREAM_SIZE,
        },
        shared_highest_block: Arc::new(RwLock::new(None)),
        pending_data: Arc::new(RwLock::new(PendingData::default())),
        central_source: Arc::new(central),
        pending_source: Arc::new(pending_source),
        pending_classes: Arc::new(RwLock::new(PendingClasses::default())),
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
    central_mock.expect_get_latest_block().returning(|| Ok(None));

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
    const LATEST_BLOCK_NUMBER: BlockNumber = BlockNumber(N_BLOCKS - 1);
    // FIXME: (Omri) analyze and set a lower value.
    const MAX_TIME_TO_SYNC_MS: u64 = 800;
    let _ = simple_logger::init_with_env();

    // Mock having N_BLOCKS chain in central.
    let mut central_mock = MockCentralSourceTrait::new();
    central_mock.expect_get_latest_block().returning(|| {
        Ok(Some(BlockHashAndNumber {
            block_number: LATEST_BLOCK_NUMBER,
            block_hash: create_block_hash(LATEST_BLOCK_NUMBER, false),
        }))
    });
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
                yield Ok((
                    block_number,
                    Block { header, body: BlockBody::default() },
                    BlockSignature::default(),
                    StarknetVersion(STARKNET_VERSION.to_string())
                ));
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
        async fn get_latest_block(&self) -> Result<Option<BlockHashAndNumber>, CentralError> {
            let already_reverted = self.revert_happend();
            let block_number = match already_reverted {
                false => BlockNumber(N_BLOCKS_BEFORE_REVERT),
                true => BlockNumber(N_BLOCKS_AFTER_REVERT),
            }
            .prev()
            .unwrap();
            Ok(Some(BlockHashAndNumber {
                block_hash: create_block_hash(block_number, already_reverted),
                block_number,
            }))
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
                            ..BlockHeader::default()
                        };
                        yield Ok((
                            i,
                            Block{ header, body: BlockBody::default() },
                            BlockSignature::default(),
                            StarknetVersion(STARKNET_VERSION.to_string()),
                        ));
                    }
                }
                .boxed(),
                true => stream! {
                    for i in initial_block_number.iter_up_to(up_to_block_number) {
                        if i.0 >= N_BLOCKS_AFTER_REVERT {
                            yield Err(CentralError::BlockNotFound { block_number: i });
                        }
                        let header = BlockHeader {
                            block_number: i,
                            block_hash: create_block_hash(i, i.0 >= CHAIN_FORK_BLOCK_NUMBER),
                            parent_hash: create_block_hash(i.prev().unwrap_or_default(), i.0 > CHAIN_FORK_BLOCK_NUMBER),
                            ..BlockHeader::default()
                        };
                        yield Ok((
                            i,
                            Block{header, body: BlockBody::default()},
                            BlockSignature::default(),
                            StarknetVersion(STARKNET_VERSION.to_string())
                        ));
                    }
                }
                .boxed()
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

        async fn get_class(
            &self,
            _class_hash: ClassHash,
        ) -> Result<ApiContractClass, CentralError> {
            unimplemented!();
        }

        async fn get_compiled_class(
            &self,
            _class_hash: ClassHash,
        ) -> Result<CasmContractClass, CentralError> {
            unimplemented!();
        }
    }
}

#[tokio::test]
async fn test_unrecoverable_sync_error_flow() {
    let _ = simple_logger::init_with_env();

    const LATEST_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
    const BLOCK_NUMBER: BlockNumber = BlockNumber(1);
    const WRONG_BLOCK_NUMBER: BlockNumber = BlockNumber(2);

    // Mock central with one block but return wrong header.
    let mut mock = MockCentralSourceTrait::new();
    mock.expect_get_latest_block().returning(|| {
        Ok(Some(BlockHashAndNumber {
            block_number: LATEST_BLOCK_NUMBER,
            block_hash: create_block_hash(LATEST_BLOCK_NUMBER, false),
        }))
    });
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
                BlockSignature::default(),
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
        BlockHash(Felt::from_hex(format!("0x{}10", bn.0).as_str()).unwrap())
    } else {
        BlockHash(Felt::from_hex(format!("0x{}", bn.0).as_str()).unwrap())
    }
}
