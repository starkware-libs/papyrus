use std::collections::HashSet;
use std::hash::Hash;
use std::pin::Pin;

use futures::future::BoxFuture;
use futures::{FutureExt, Stream, StreamExt};
use indexmap::IndexMap;
use papyrus_network::DataType;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, StateDiff, ThinStateDiff};

use crate::stream_factory::{BlockData, BlockNumberLimit, DataStreamFactory};
use crate::{P2PSyncError, NETWORK_DATA_TIMEOUT};

impl BlockData for (ThinStateDiff, BlockNumber) {
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        // TODO(shahak): Add a way to write ThinStateDiff to the storage.
        let state_diff = StateDiff {
            deployed_contracts: self.0.deployed_contracts,
            storage_diffs: self.0.storage_diffs,
            declared_classes: self
                .0
                .declared_classes
                .into_iter()
                .map(|(class_hash, compiled_class_hash)| {
                    (class_hash, (compiled_class_hash, ContractClass::default()))
                })
                .collect(),
            deprecated_declared_classes: self
                .0
                .deprecated_declared_classes
                .into_iter()
                .map(|class_hash| (class_hash, DeprecatedContractClass::default()))
                .collect(),
            nonces: self.0.nonces,
            replaced_classes: self.0.replaced_classes,
        };

        storage_writer
            .begin_rw_txn()?
            .append_state_diff(self.1, state_diff, IndexMap::new())?
            .commit()
    }
}

pub(crate) struct StateDiffStreamFactory;

impl DataStreamFactory for StateDiffStreamFactory {
    type InputFromNetwork = ThinStateDiff;
    type Output = (ThinStateDiff, BlockNumber);

    const DATA_TYPE: DataType = DataType::StateDiff;
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;
    const SHOULD_LOG_ADDED_BLOCK: bool = true;

    fn parse_data_for_block<'a>(
        state_diffs_receiver: &'a mut Pin<
            Box<dyn Stream<Item = Option<Self::InputFromNetwork>> + Send>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>> {
        async move {
            let mut result = ThinStateDiff::default();
            let mut prev_result_len = 0;
            let mut result_len = 0;
            let target_len = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_number)?
                .expect("A header with number lower than the header marker is missing")
                .state_diff_length
                .ok_or(P2PSyncError::OldHeaderInStorage {
                    block_number,
                    missing_field: "state_diff_length",
                })?;

            while result_len < target_len {
                let Some(maybe_state_diff_part) =
                    tokio::time::timeout(NETWORK_DATA_TIMEOUT, state_diffs_receiver.next()).await?
                else {
                    return Err(P2PSyncError::ReceiverChannelTerminated {
                        data_type: Self::DATA_TYPE,
                    });
                };
                let Some(state_diff_part) = maybe_state_diff_part else {
                    if result_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(P2PSyncError::WrongStateDiffLength {
                            expected_length: target_len,
                            possible_lengths: vec![result_len],
                        });
                    }
                };
                prev_result_len = result_len;
                if state_diff_part.is_empty() {
                    return Err(P2PSyncError::EmptyStateDiffPart);
                }
                // It's cheaper to calculate the length of `state_diff_part` than the length of
                // `result`.
                result_len += state_diff_part.len();
                unite_state_diffs(&mut result, state_diff_part)?;
            }

            if result_len != target_len {
                return Err(P2PSyncError::WrongStateDiffLength {
                    expected_length: target_len,
                    possible_lengths: vec![prev_result_len, result_len],
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
