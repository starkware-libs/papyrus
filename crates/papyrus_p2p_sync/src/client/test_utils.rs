use std::time::Duration;

use futures::channel::mpsc::Receiver;
use lazy_static::lazy_static;
use papyrus_network::network_manager::SqmrClientPayload;
use papyrus_protobuf::sync::{
    DataOrFin,
    HeaderQuery,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockSignature};
use starknet_api::crypto::utils::Signature;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{Transaction, TransactionOutput};
use starknet_types_core::felt::Felt;

use super::{P2PSyncClient, P2PSyncClientChannels, P2PSyncClientConfig};

pub const BUFFER_SIZE: usize = 1000;
pub const HEADER_QUERY_LENGTH: u64 = 5;
pub const STATE_DIFF_QUERY_LENGTH: u64 = 3;
pub const SLEEP_DURATION_TO_LET_SYNC_ADVANCE: Duration = Duration::from_millis(10);
// This should be substantially bigger than SLEEP_DURATION_TO_LET_SYNC_ADVANCE.
pub const WAIT_PERIOD_FOR_NEW_DATA: Duration = Duration::from_millis(50);
pub const TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE: Duration =
    WAIT_PERIOD_FOR_NEW_DATA.saturating_add(SLEEP_DURATION_TO_LET_SYNC_ADVANCE.saturating_mul(10));

lazy_static! {
    static ref TEST_CONFIG: P2PSyncClientConfig = P2PSyncClientConfig {
        num_headers_per_query: HEADER_QUERY_LENGTH,
        num_block_state_diffs_per_query: STATE_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        buffer_size: BUFFER_SIZE,
        stop_sync_at_block_number: None,
    };
}

pub struct TestArgs {
    #[allow(clippy::type_complexity)]
    pub p2p_sync: P2PSyncClient,
    pub storage_reader: StorageReader,
    pub header_payload_receiver:
        Receiver<SqmrClientPayload<HeaderQuery, DataOrFin<SignedBlockHeader>>>,
    pub state_diff_payload_receiver:
        Receiver<SqmrClientPayload<StateDiffQuery, DataOrFin<StateDiffChunk>>>,
    #[allow(dead_code)]
    pub transaction_payload_receiver:
        Receiver<SqmrClientPayload<TransactionQuery, DataOrFin<(Transaction, TransactionOutput)>>>,
}

pub fn setup() -> TestArgs {
    let p2p_sync_config = *TEST_CONFIG;
    let buffer_size = p2p_sync_config.buffer_size;
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (header_payload_sender, header_payload_receiver) =
        futures::channel::mpsc::channel(buffer_size);
    let (state_diff_payload_sender, state_diff_payload_receiver) =
        futures::channel::mpsc::channel(buffer_size);
    let (transaction_payload_sender, transaction_payload_receiver) =
        futures::channel::mpsc::channel(buffer_size);
    let p2p_sync_channels = P2PSyncClientChannels {
        header_payload_sender: Box::new(header_payload_sender),
        state_diff_payload_sender: Box::new(state_diff_payload_sender),
        transaction_payload_sender: Box::new(transaction_payload_sender),
    };
    let p2p_sync = P2PSyncClient::new(
        p2p_sync_config,
        storage_reader.clone(),
        storage_writer,
        p2p_sync_channels,
    );
    TestArgs {
        p2p_sync,
        storage_reader,
        header_payload_receiver,
        state_diff_payload_receiver,
        transaction_payload_receiver,
    }
}

pub fn create_block_hashes_and_signatures(n_blocks: u8) -> Vec<(BlockHash, BlockSignature)> {
    let mut bytes = [0u8; 32];
    (0u8..n_blocks)
        .map(|i| {
            bytes[31] = i;
            (
                BlockHash(StarkHash::from_bytes_be(&bytes)),
                BlockSignature(Signature {
                    r: Felt::from_bytes_be(&bytes),
                    s: Felt::from_bytes_be(&bytes),
                }),
            )
        })
        .collect()
}
