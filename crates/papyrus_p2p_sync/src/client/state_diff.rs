use std::collections::HashSet;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use papyrus_proc_macros::latency_histogram;
use papyrus_protobuf::sync::StateDiffChunk;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;

use super::ResponseReceiver;
use crate::client::stream_factory::{BlockData, BlockNumberLimit, DataStreamFactory};
use crate::client::{P2PSyncError, NETWORK_DATA_TIMEOUT};

impl BlockData for (ThinStateDiff, BlockNumber) {
    #[latency_histogram("p2p_sync_state_diff_write_to_storage_latency_seconds", true)]
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        storage_writer.begin_rw_txn()?.append_state_diff(self.1, self.0)?.commit()
    }
}

pub(crate) struct StateDiffStreamFactory;

impl DataStreamFactory<StateDiffChunk> for StateDiffStreamFactory {
    type Output = (ThinStateDiff, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "state diffs";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;

    #[latency_histogram("p2p_sync_state_diff_parse_data_for_block_latency_seconds", true)]
    fn parse_data_for_block<'a>(
        state_diff_chunks_receiver: &'a mut ResponseReceiver<StateDiffChunk>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>> {
        async move {
            let mut result = ThinStateDiff::default();
            let mut prev_result_len = 0;
            let mut current_state_diff_len = 0;
            let target_state_diff_len = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_number)?
                .expect("A header with number lower than the header marker is missing")
                .state_diff_length
                .ok_or(P2PSyncError::OldHeaderInStorage {
                    block_number,
                    missing_field: "state_diff_length",
                })?;

            while current_state_diff_len < target_state_diff_len {
                let (maybe_state_diff_chunk, _report_callback) =
                    tokio::time::timeout(NETWORK_DATA_TIMEOUT, state_diff_chunks_receiver.next())
                        .await?
                        .ok_or(P2PSyncError::ReceiverChannelTerminated {
                            type_description: Self::TYPE_DESCRIPTION,
                        })?;
                let Some(state_diff_chunk) = maybe_state_diff_chunk?.0 else {
                    if current_state_diff_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(P2PSyncError::WrongStateDiffLength {
                            expected_length: target_state_diff_len,
                            possible_lengths: vec![current_state_diff_len],
                        });
                    }
                };
                prev_result_len = current_state_diff_len;
                if state_diff_chunk.is_empty() {
                    return Err(P2PSyncError::EmptyStateDiffPart);
                }
                // It's cheaper to calculate the length of `state_diff_part` than the length of
                // `result`.
                current_state_diff_len += state_diff_chunk.len();
                unite_state_diffs(&mut result, state_diff_chunk)?;
            }

            if current_state_diff_len != target_state_diff_len {
                return Err(P2PSyncError::WrongStateDiffLength {
                    expected_length: target_state_diff_len,
                    possible_lengths: vec![prev_result_len, current_state_diff_len],
                });
            }

            validate_deprecated_declared_classes_non_conflicting(&result)?;
            Ok(Some((result, block_number)))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_state_marker()
    }
}

// For performance reasons, this function does not check if a deprecated class was declared twice.
// That check is done after we get the final state diff.
#[latency_histogram("p2p_sync_state_diff_unite_state_diffs_latency_seconds", true)]
fn unite_state_diffs(
    state_diff: &mut ThinStateDiff,
    state_diff_chunk: StateDiffChunk,
) -> Result<(), P2PSyncError> {
    match state_diff_chunk {
        StateDiffChunk::ContractDiff(contract_diff) => {
            if let Some(class_hash) = contract_diff.class_hash {
                if state_diff
                    .deployed_contracts
                    .insert(contract_diff.contract_address, class_hash)
                    .is_some()
                {
                    return Err(P2PSyncError::ConflictingStateDiffParts);
                }
            }
            if let Some(nonce) = contract_diff.nonce {
                if state_diff.nonces.insert(contract_diff.contract_address, nonce).is_some() {
                    return Err(P2PSyncError::ConflictingStateDiffParts);
                }
            }
            match state_diff.storage_diffs.get_mut(&contract_diff.contract_address) {
                Some(storage_diffs) => {
                    for (k, v) in contract_diff.storage_diffs {
                        if storage_diffs.insert(k, v).is_some() {
                            return Err(P2PSyncError::ConflictingStateDiffParts);
                        }
                    }
                }
                None => {
                    state_diff
                        .storage_diffs
                        .insert(contract_diff.contract_address, contract_diff.storage_diffs);
                }
            }
        }
        StateDiffChunk::DeclaredClass(declared_class) => {
            if state_diff
                .declared_classes
                .insert(declared_class.class_hash, declared_class.compiled_class_hash)
                .is_some()
            {
                return Err(P2PSyncError::ConflictingStateDiffParts);
            }
        }
        StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
            state_diff.deprecated_declared_classes.push(deprecated_declared_class.class_hash);
        }
    }
    Ok(())
}

#[latency_histogram(
    "p2p_sync_state_diff_validate_deprecated_declared_classes_non_conflicting_latency_seconds",
    true
)]
fn validate_deprecated_declared_classes_non_conflicting(
    state_diff: &ThinStateDiff,
) -> Result<(), P2PSyncError> {
    // TODO(shahak): Check if sorting is more efficient.
    if state_diff.deprecated_declared_classes.len()
        == state_diff.deprecated_declared_classes.iter().cloned().collect::<HashSet<_>>().len()
    {
        Ok(())
    } else {
        Err(P2PSyncError::ConflictingStateDiffParts)
    }
}
