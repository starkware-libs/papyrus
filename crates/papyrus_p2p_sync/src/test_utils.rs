use std::time::Duration;

use futures::channel::mpsc::{Receiver, Sender};
use lazy_static::lazy_static;
use papyrus_protobuf::sync::{HeaderQuery, SignedBlockHeader, StateDiffQuery};
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockSignature};
use starknet_api::crypto::Signature;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::ThinStateDiff;

use crate::{P2PSync, P2PSyncConfig, Response};

pub const BUFFER_SIZE: usize = 1000;
pub const HEADER_QUERY_LENGTH: u64 = 5;
pub const STATE_DIFF_QUERY_LENGTH: u64 = 3;
pub const SLEEP_DURATION_TO_LET_SYNC_ADVANCE: Duration = Duration::from_millis(10);
// This should be substantially bigger than SLEEP_DURATION_TO_LET_SYNC_ADVANCE.
pub const WAIT_PERIOD_FOR_NEW_DATA: Duration = Duration::from_millis(50);
pub const TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE: Duration =
    WAIT_PERIOD_FOR_NEW_DATA.saturating_add(SLEEP_DURATION_TO_LET_SYNC_ADVANCE.saturating_mul(10));

lazy_static! {
    static ref TEST_CONFIG: P2PSyncConfig = P2PSyncConfig {
        num_headers_per_query: HEADER_QUERY_LENGTH,
        num_block_state_diffs_per_query: STATE_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        stop_sync_at_block_number: None,
    };
}

pub struct TestArgs {
    #[allow(clippy::type_complexity)]
    pub p2p_sync: P2PSync<
        Sender<HeaderQuery>,
        Receiver<Response<SignedBlockHeader>>,
        Sender<StateDiffQuery>,
        Receiver<Response<ThinStateDiff>>,
    >,
    pub storage_reader: StorageReader,
    pub header_query_receiver: Receiver<HeaderQuery>,
    pub state_diff_query_receiver: Receiver<StateDiffQuery>,
    pub headers_sender: Sender<Response<SignedBlockHeader>>,
    pub state_diffs_sender: Sender<Response<ThinStateDiff>>,
}

pub fn setup() -> TestArgs {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (header_query_sender, header_query_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let (state_diff_query_sender, state_diff_query_receiver) =
        futures::channel::mpsc::channel(BUFFER_SIZE);
    let (headers_sender, headers_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let (state_diffs_sender, state_diffs_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let p2p_sync = P2PSync::new(
        *TEST_CONFIG,
        storage_reader.clone(),
        storage_writer,
        header_query_sender,
        headers_receiver,
        state_diff_query_sender,
        state_diffs_receiver,
    );
    TestArgs {
        p2p_sync,
        storage_reader,
        header_query_receiver,
        state_diff_query_receiver,
        headers_sender,
        state_diffs_sender,
    }
}

pub fn create_block_hashes_and_signatures(n_blocks: u8) -> Vec<(BlockHash, BlockSignature)> {
    let mut bytes = [0u8; 32];
    (0u8..n_blocks)
        .map(|i| {
            bytes[31] = i;
            (
                BlockHash(StarkHash::new(bytes).unwrap()),
                BlockSignature(Signature {
                    r: StarkFelt::new(bytes).unwrap(),
                    s: StarkFelt::new(bytes).unwrap(),
                }),
            )
        })
        .collect()
}
