mod header;
#[cfg(test)]
mod p2p_sync_test;
mod state_diff;
mod stream_factory;

use std::collections::BTreeMap;
use std::time::Duration;

use futures::channel::mpsc::{SendError, Sender};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::{DataType, Query, ResponseReceivers};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockSignature};
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::header::HeaderStreamFactory;
use crate::state_diff::StateDiffStreamFactory;
use crate::stream_factory::DataStreamFactory;

const STEP: usize = 1;
const ALLOWED_SIGNATURES_LENGTH: usize = 1;

const NETWORK_DATA_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PSyncConfig {
    pub num_headers_per_query: usize,
    pub num_block_state_diffs_per_query: usize,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub wait_period_for_new_data: Duration,
}

impl SerializeConfig for P2PSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "num_headers_per_query",
                &self.num_headers_per_query,
                "The maximum amount of headers to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_block_state_diffs_per_query",
                &self.num_block_state_diffs_per_query,
                "The maximum amount of block's state diffs to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "wait_period_for_new_data",
                &self.wait_period_for_new_data.as_secs(),
                "Time in seconds to wait when a query returned with partial data before sending a \
                 new query",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for P2PSyncConfig {
    fn default() -> Self {
        P2PSyncConfig {
            num_headers_per_query: 10000,
            // State diffs are split into multiple messages, so big queries can lead to a lot of
            // messages in the network buffers.
            num_block_state_diffs_per_query: 100,
            wait_period_for_new_data: Duration::from_secs(5),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum P2PSyncError {
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    // TODO(shahak): Consider removing this error and handling unordered headers without failing.
    #[error(
        "Blocks returned unordered from the network. Expected header with \
         {expected_block_number}, got {actual_block_number}."
    )]
    HeadersUnordered { expected_block_number: BlockNumber, actual_block_number: BlockNumber },
    #[error("Expected to receive one signature from the network. got {signatures:?} instead.")]
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    // Right now we support only one signature. In the future we will support many signatures.
    WrongSignaturesLength { signatures: Vec<BlockSignature> },
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error(
        "Expected state diff of length {expected_length}. The possible options for state diff \
         length got are {possible_lengths:?}."
    )]
    WrongStateDiffLength { expected_length: usize, possible_lengths: Vec<usize> },
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error("Two state diff parts for the same state diff are conflicting.")]
    ConflictingStateDiffParts,
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error(
        "Received an empty state diff part from the network (this is a potential DDoS vector)."
    )]
    EmptyStateDiffPart,
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error("Network returned more responses than expected for a query.")]
    TooManyResponses,
    #[error(
        "Encountered an old header in the storage at {block_number:?} that's missing the field \
         {missing_field}"
    )]
    OldHeaderInStorage { block_number: BlockNumber, missing_field: &'static str },
    #[error("The sender end of the response receivers for {data_type:?} was closed.")]
    ReceiverChannelTerminated { data_type: DataType },
    #[error(transparent)]
    NetworkTimeout(#[from] tokio::time::error::Elapsed),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    SendError(#[from] SendError),
}

pub struct P2PSync {
    config: P2PSyncConfig,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    query_sender: Sender<Query>,
    response_receivers: ResponseReceivers,
}

impl P2PSync {
    pub fn new(
        config: P2PSyncConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        query_sender: Sender<Query>,
        response_receivers: ResponseReceivers,
    ) -> Self {
        Self { config, storage_reader, storage_writer, query_sender, response_receivers }
    }

    #[instrument(skip(self), level = "debug", err)]
    pub async fn run(mut self) -> Result<(), P2PSyncError> {
        let header_stream = HeaderStreamFactory::create_stream(
            self.response_receivers
                .signed_headers_receiver
                .expect("p2p sync needs a signed headers receiver"),
            self.query_sender.clone(),
            self.storage_reader.clone(),
            self.config.wait_period_for_new_data,
            self.config.num_headers_per_query,
        );

        let state_diff_stream = StateDiffStreamFactory::create_stream(
            self.response_receivers
                .state_diffs_receiver
                .expect("p2p sync needs a state diffs receiver"),
            self.query_sender,
            self.storage_reader,
            self.config.wait_period_for_new_data,
            self.config.num_block_state_diffs_per_query,
        );

        let mut data_stream = header_stream.merge(state_diff_stream);

        loop {
            let data = data_stream.next().await.expect("Sync data stream should never end")?;
            data.write_to_storage(&mut self.storage_writer)?;
        }
    }
}
