#[cfg(test)]
mod p2p_sync_test;

use std::collections::BTreeMap;
use std::pin::Pin;
use std::time::Duration;

use async_stream::stream;
use futures::channel::mpsc::{SendError, Sender};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, SinkExt, Stream, StreamExt};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::{DataType, Direction, Query, ResponseReceivers, SignedBlockHeader};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockNumber, BlockSignature};
use tracing::{debug, info, instrument};

const STEP: usize = 1;
const ALLOWED_SIGNATURES_LENGTH: usize = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PSyncConfig {
    pub num_headers_per_query: usize,
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
    // TODO(shahak): Move this error to network.
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    WrongSignaturesLength { signatures: Vec<BlockSignature> },
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error("Network returned more responses than expected for a query.")]
    TooManyResponses,
    // TODO(shahak): Replicate this error for each data type.
    #[error("The sender end of the response receivers was closed.")]
    ReceiverChannelTerminated,
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
        let mut data_stream = HeaderStreamFactory::create_stream(
            self.response_receivers.signed_headers_receiver,
            self.query_sender,
            self.storage_reader,
            self.config.wait_period_for_new_data,
            self.config.num_headers_per_query,
        );

        loop {
            let data = data_stream.next().await.expect("Sync data stream should never end")?;
            data.write_to_storage(&mut self.storage_writer)?;
        }
    }
}

trait BlockData: Send {
    fn write_to_storage(&self, storage_writer: &mut StorageWriter) -> Result<(), StorageError>;
}

struct HeaderData {
    pub block_number: BlockNumber,
    pub block_header: BlockHeader,
    pub block_signature: BlockSignature,
}

impl BlockData for HeaderData {
    fn write_to_storage(&self, storage_writer: &mut StorageWriter) -> Result<(), StorageError> {
        storage_writer
            .begin_rw_txn()?
            .append_header(self.block_number, &self.block_header)?
            .append_block_signature(self.block_number, &self.block_signature)?
            .commit()
    }
}

enum BlockNumberLimit {
    Unlimited,
    // TODO(shahak): Add variant for header marker once we support state diff sync.
    // TODO(shahak): Add variant for state diff marker once we support classes sync.
}

trait DataStreamFactory {
    type InputFromNetwork: Send + 'static;
    type Output: BlockData + 'static;

    const DATA_TYPE: DataType;
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit;
    const SHOULD_LOG_ADDED_BLOCK: bool;

    // Async functions in trait don't work well with argument references
    fn parse_data_for_block<'a>(
        data_receiver: &'a mut Pin<Box<dyn Stream<Item = Option<Self::InputFromNetwork>> + Send>>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>>;

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError>;

    fn create_stream(
        mut data_receiver: Pin<Box<dyn Stream<Item = Option<Self::InputFromNetwork>> + Send>>,
        mut query_sender: Sender<Query>,
        storage_reader: StorageReader,
        wait_period_for_new_data: Duration,
        num_blocks_per_query: usize,
    ) -> BoxStream<'static, Result<Box<dyn BlockData>, P2PSyncError>> {
        stream! {
            let mut current_block_number = Self::get_start_block_number(&storage_reader)?;
            'send_query_and_parse_responses: loop {
                let end_block_number = current_block_number.0
                    + u64::try_from(num_blocks_per_query)
                        .expect("Failed converting usize to u64");
                debug!("Downloading blocks [{}, {})", current_block_number.0, end_block_number);
                query_sender
                    .send(Query {
                        start_block: current_block_number,
                        direction: Direction::Forward,
                        limit: num_blocks_per_query,
                        step: STEP,
                        data_type: Self::DATA_TYPE,
                    })
                    .await?;

                while current_block_number.0 < end_block_number {
                    match Self::parse_data_for_block(
                        &mut data_receiver, current_block_number, &storage_reader
                    ).await? {
                        Some(output) => yield Ok(Box::<dyn BlockData>::from(Box::new(output))),
                        None => {
                            debug!(
                                "Query for {:?} returned with partial data. Waiting {:?} before \
                                 sending another query.",
                                Self::DATA_TYPE,
                                wait_period_for_new_data
                            );
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue 'send_query_and_parse_responses;
                        }
                    }
                    if Self::SHOULD_LOG_ADDED_BLOCK {
                        info!("Added block {}.", current_block_number);
                    }
                    current_block_number = current_block_number.next();
                }

                // Consume the None message signaling the end of the query.
                match data_receiver.next().await {
                    Some(None) => {},
                    Some(Some(_)) => Err(P2PSyncError::TooManyResponses)?,
                    None => Err(P2PSyncError::ReceiverChannelTerminated)?,
                }
            }
        }
        .boxed()
    }
}

struct HeaderStreamFactory;

impl DataStreamFactory for HeaderStreamFactory {
    type InputFromNetwork = SignedBlockHeader;
    type Output = HeaderData;

    const DATA_TYPE: DataType = DataType::SignedBlockHeader;
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::Unlimited;
    const SHOULD_LOG_ADDED_BLOCK: bool = true;

    fn parse_data_for_block<'a>(
        signed_headers_receiver: &'a mut Pin<
            Box<dyn Stream<Item = Option<Self::InputFromNetwork>> + Send>,
        >,
        block_number: BlockNumber,
        _storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>> {
        async move {
            let maybe_signed_header_stream_result = signed_headers_receiver.next().await;
            let Some(maybe_signed_header) = maybe_signed_header_stream_result else {
                return Err(P2PSyncError::ReceiverChannelTerminated);
            };
            let Some(SignedBlockHeader { block_header, signatures }) = maybe_signed_header else {
                debug!("Header query sent to network finished");
                return Ok(None);
            };
            // TODO(shahak): Check that parent_hash is the same as the previous block's hash
            // and handle reverts.
            if block_number != block_header.block_number {
                return Err(P2PSyncError::HeadersUnordered {
                    expected_block_number: block_number,
                    actual_block_number: block_header.block_number,
                });
            }
            if signatures.len() != ALLOWED_SIGNATURES_LENGTH {
                return Err(P2PSyncError::WrongSignaturesLength { signatures });
            }
            Ok(Some(HeaderData {
                block_number,
                block_header,
                block_signature: signatures.into_iter().next().expect(
                    "Calling first on a vector of size {ALLOWED_SIGNATURES_LENGTH} returned None",
                ),
            }))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_header_marker()
    }
}
