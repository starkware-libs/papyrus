mod header;
#[cfg(test)]
mod header_test;
mod state_diff;
#[cfg(test)]
mod state_diff_test;
mod stream_factory;
#[cfg(test)]
mod test_utils;

use std::collections::BTreeMap;
use std::time::Duration;

use futures::channel::mpsc::SendError;
use futures::future::ready;
use futures::{Sink, SinkExt, Stream};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::network_manager::ReportCallback;
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{DataOrFin, HeaderQuery, SignedBlockHeader, StateDiffQuery};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockSignature};
use starknet_api::state::ThinStateDiff;
use tokio_stream::StreamExt;
use tracing::instrument;

use crate::header::HeaderStreamFactory;
use crate::state_diff::StateDiffStreamFactory;
use crate::stream_factory::DataStreamFactory;

const STEP: u64 = 1;
const ALLOWED_SIGNATURES_LENGTH: usize = 1;

const NETWORK_DATA_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PSyncConfig {
    pub num_headers_per_query: u64,
    pub num_block_state_diffs_per_query: u64,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub wait_period_for_new_data: Duration,
    pub stop_sync_at_block_number: Option<BlockNumber>,
}

impl SerializeConfig for P2PSyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
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
        ]);
        config.extend(ser_optional_param(
            &self.stop_sync_at_block_number,
            BlockNumber(1000),
            "stop_sync_at_block_number",
            "Stops the sync at given block number and closes the node cleanly. Used to run \
             profiling on the node.",
            ParamPrivacyInput::Public,
        ));
        config
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
            stop_sync_at_block_number: None,
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
        "The header says that the block's state diff should be of length {expected_length}. Can \
         only divide the state diff parts into the following lengths: {possible_lengths:?}."
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
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    #[error(
        "Encountered an old header in the storage at {block_number:?} that's missing the field \
         {missing_field}. Re-sync the node from {block_number:?} from a node that provides this \
         field."
    )]
    OldHeaderInStorage { block_number: BlockNumber, missing_field: &'static str },
    #[error("The sender end of the response receivers for {type_description:?} was closed.")]
    ReceiverChannelTerminated { type_description: &'static str },
    #[error(transparent)]
    NetworkTimeout(#[from] tokio::time::error::Elapsed),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    SendError(#[from] SendError),
}

type Response<T> = (Result<DataOrFin<T>, ProtobufConversionError>, ReportCallback);

pub struct P2PSync<
    HeaderQuerySender,
    HeaderResponseReceiver,
    StateDiffQuerySender,
    StateDiffResponseReceiver,
> {
    config: P2PSyncConfig,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    header_query_sender: HeaderQuerySender,
    header_response_receiver: HeaderResponseReceiver,
    state_diff_query_sender: StateDiffQuerySender,
    state_diff_response_receiver: StateDiffResponseReceiver,
}

impl<HeaderQuerySender, HeaderResponseReceiver, StateDiffQuerySender, StateDiffResponseReceiver>
    P2PSync<
        HeaderQuerySender,
        HeaderResponseReceiver,
        StateDiffQuerySender,
        StateDiffResponseReceiver,
    >
where
    HeaderQuerySender: Sink<HeaderQuery, Error = SendError> + Unpin + Send + 'static,
    HeaderResponseReceiver: Stream<Item = Response<SignedBlockHeader>> + Unpin + Send + 'static,
    StateDiffQuerySender: Sink<StateDiffQuery, Error = SendError> + Unpin + Send + 'static,
    // TODO(shahak): Change to StateDiffChunk.
    StateDiffResponseReceiver: Stream<Item = Response<ThinStateDiff>> + Unpin + Send + 'static,
{
    pub fn new(
        config: P2PSyncConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        header_query_sender: HeaderQuerySender,
        header_response_receiver: HeaderResponseReceiver,
        state_diff_query_sender: StateDiffQuerySender,
        state_diff_response_receiver: StateDiffResponseReceiver,
    ) -> Self {
        Self {
            config,
            storage_reader,
            storage_writer,
            header_query_sender,
            header_response_receiver,
            state_diff_query_sender,
            state_diff_response_receiver,
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    pub async fn run(mut self) -> Result<(), P2PSyncError> {
        let header_stream = HeaderStreamFactory::create_stream(
            self.header_query_sender.with(|query| ready(Ok(HeaderQuery(query)))),
            self.header_response_receiver,
            self.storage_reader.clone(),
            self.config.wait_period_for_new_data,
            self.config.num_headers_per_query,
            self.config.stop_sync_at_block_number,
        );

        let state_diff_stream = StateDiffStreamFactory::create_stream(
            self.state_diff_query_sender.with(|query| ready(Ok(StateDiffQuery(query)))),
            self.state_diff_response_receiver,
            self.storage_reader,
            self.config.wait_period_for_new_data,
            self.config.num_block_state_diffs_per_query,
            self.config.stop_sync_at_block_number,
        );

        let mut data_stream = header_stream.merge(state_diff_stream);

        loop {
            let data = data_stream.next().await.expect("Sync data stream should never end")?;
            data.write_to_storage(&mut self.storage_writer)?;
        }
    }
}
