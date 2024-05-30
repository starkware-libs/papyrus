use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;

use futures::channel::mpsc::SendError;
use futures::future::BoxFuture;
use futures::{FutureExt, Sink, Stream, StreamExt};
use indexmap::IndexMap;
use papyrus_proc_macros::latency_histogram;
use papyrus_protobuf::sync::Query;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;

use crate::stream_factory::{BlockData, BlockNumberLimit, DataStreamFactory};
use crate::{P2PSyncError, Response, NETWORK_DATA_TIMEOUT};

impl BlockData for (ThinStateDiff, BlockNumber) {
    #[latency_histogram("p2p_sync_state_diff_write_to_storage_latency_seconds", true)]
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        storage_writer.begin_rw_txn()?.append_state_diff(self.1, self.0)?.commit()
    }
}

pub(crate) struct StateDiffStreamFactory<QuerySender, DataReceiver>(
    PhantomData<(QuerySender, DataReceiver)>,
);

// TODO(shahak): Change to StateDiffChunk.
impl<QuerySender, DataReceiver> DataStreamFactory<QuerySender, DataReceiver, ThinStateDiff>
    for StateDiffStreamFactory<QuerySender, DataReceiver>
where
    QuerySender: Sink<Query, Error = SendError> + Unpin + Send + 'static,
    DataReceiver: Stream<Item = Response<ThinStateDiff>> + Unpin + Send + 'static,
{
    type Output = (ThinStateDiff, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "state diffs";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;

    #[latency_histogram("p2p_sync_state_diff_parse_data_for_block_latency_seconds", true)]
    fn parse_data_for_block<'a>(
        state_diffs_receiver: &'a mut DataReceiver,
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
                let (maybe_state_diff_part, _report_callback) =
                    tokio::time::timeout(NETWORK_DATA_TIMEOUT, state_diffs_receiver.next())
                        .await?
                        .ok_or(P2PSyncError::ReceiverChannelTerminated {
                            type_description: Self::TYPE_DESCRIPTION,
                        })?;
                let Some(state_diff_part) = maybe_state_diff_part?.0 else {
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
                if state_diff_part.is_empty() {
                    return Err(P2PSyncError::EmptyStateDiffPart);
                }
                // It's cheaper to calculate the length of `state_diff_part` than the length of
                // `result`.
                current_state_diff_len += state_diff_part.len();
                unite_state_diffs(&mut result, state_diff_part)?;
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
    other_state_diff: ThinStateDiff,
) -> Result<(), P2PSyncError> {
    unite_state_diffs_field(
        &mut state_diff.deployed_contracts,
        other_state_diff.deployed_contracts,
    )?;
    unite_state_diffs_field(&mut state_diff.declared_classes, other_state_diff.declared_classes)?;
    unite_state_diffs_field(&mut state_diff.nonces, other_state_diff.nonces)?;
    unite_state_diffs_field(&mut state_diff.replaced_classes, other_state_diff.replaced_classes)?;

    for (other_contract_address, other_storage_diffs) in other_state_diff.storage_diffs {
        match state_diff.storage_diffs.get_mut(&other_contract_address) {
            Some(storage_diffs) => unite_state_diffs_field(storage_diffs, other_storage_diffs)?,
            None => {
                state_diff.storage_diffs.insert(other_contract_address, other_storage_diffs);
            }
        }
    }

    state_diff.deprecated_declared_classes.extend(other_state_diff.deprecated_declared_classes);
    Ok(())
}

fn unite_state_diffs_field<K: Hash + Eq + PartialEq, V>(
    field: &mut IndexMap<K, V>,
    other_field: IndexMap<K, V>,
) -> Result<(), P2PSyncError> {
    for (k, v) in other_field {
        let insert_result = field.insert(k, v);
        if insert_result.is_some() {
            return Err(P2PSyncError::ConflictingStateDiffParts);
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
