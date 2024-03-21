const BUFFER_SIZE: usize = 1000;
const HEADER_QUERY_LENGTH: usize = 5;
const STATE_DIFF_QUERY_LENGTH: usize = 3;
const SLEEP_DURATION_TO_LET_SYNC_ADVANCE: Duration = Duration::from_millis(10);
// This should be substantially bigger than SLEEP_DURATION_TO_LET_SYNC_ADVANCE.
const WAIT_PERIOD_FOR_NEW_DATA: Duration = Duration::from_millis(50);
const TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE: Duration =
    WAIT_PERIOD_FOR_NEW_DATA.saturating_add(SLEEP_DURATION_TO_LET_SYNC_ADVANCE.saturating_mul(10));

lazy_static! {
    static ref TEST_CONFIG: P2PSyncConfig = P2PSyncConfig {
        num_headers_per_query: HEADER_QUERY_LENGTH,
        num_block_state_diffs_per_query: STATE_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA
    };
}

#[allow(clippy::type_complexity)]
fn setup() -> (
    P2PSync,
    StorageReader,
    Receiver<Query>,
    Sender<Option<SignedBlockHeader>>,
    Sender<Option<ThinStateDiff>>,
) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (query_sender, query_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let (signed_headers_sender, signed_headers_receiver) =
        futures::channel::mpsc::channel(BUFFER_SIZE);
    let (state_diffs_sender, state_diffs_receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let p2p_sync = P2PSync::new(
        *TEST_CONFIG,
        storage_reader.clone(),
        storage_writer,
        query_sender,
        ResponseReceivers {
            signed_headers_receiver: Some(signed_headers_receiver.boxed()),
            state_diffs_receiver: Some(state_diffs_receiver.boxed()),
        },
    );
    (p2p_sync, storage_reader, query_receiver, signed_headers_sender, state_diffs_sender)
}

fn create_block_hashes_and_signatures(n_blocks: u8) -> Vec<(BlockHash, BlockSignature)> {
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
