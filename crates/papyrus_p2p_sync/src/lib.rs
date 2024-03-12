#[cfg(test)]
mod p2p_sync_test;

use std::collections::BTreeMap;
use std::time::Duration;

use futures::channel::mpsc::{SendError, Sender};
use futures::{SinkExt, StreamExt};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::{DataType, Direction, Query, ResponseReceivers, SignedBlockHeader};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockSignature};
use tracing::{debug, info, instrument};

const STEP: usize = 1;
const ALLOWED_SIGNATURES_LENGTH: usize = 1;

const NETWORK_DATA_TIMEOUT: Duration = Duration::from_secs(300);

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
    NetworkTimeout(#[from] tokio::time::error::Elapsed),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    SendError(#[from] SendError),
}

enum P2PSyncControl {
    ContinueDownloading,
    QueryFinishedPartially,
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
        let mut current_block_number = self.storage_reader.begin_ro_txn()?.get_header_marker()?;
        // TODO: make control more substantial once we have more peers and peer management.
        let mut control = P2PSyncControl::ContinueDownloading;
        loop {
            if matches!(control, P2PSyncControl::QueryFinishedPartially) {
                debug!(
                    "Query returned with partial data. Waiting {:?} before sending another query.",
                    self.config.wait_period_for_new_data
                );
                tokio::time::sleep(self.config.wait_period_for_new_data).await;
            }
            let end_block_number = current_block_number.0
                + u64::try_from(self.config.num_headers_per_query)
                    .expect("Failed converting usize to u64");
            debug!("Downloading blocks [{}, {})", current_block_number.0, end_block_number);
            self.query_sender
                .send(Query {
                    start_block: current_block_number,
                    direction: Direction::Forward,
                    limit: self.config.num_headers_per_query,
                    step: STEP,
                    data_type: DataType::SignedBlockHeader,
                })
                .await?;
            control = self.parse_headers(&mut current_block_number, end_block_number).await?;
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    async fn parse_headers(
        &mut self,
        current_block_number: &mut BlockNumber,
        end_block_number: u64,
    ) -> Result<P2PSyncControl, P2PSyncError> {
        while current_block_number.0 < end_block_number {
            let maybe_signed_header_stream_result = tokio::time::timeout(
                NETWORK_DATA_TIMEOUT,
                self.response_receivers.signed_headers_receiver.next(),
            )
            .await?;
            let Some(maybe_signed_header) = maybe_signed_header_stream_result else {
                return Err(P2PSyncError::ReceiverChannelTerminated);
            };
            let Some(SignedBlockHeader { block_header, signatures }) = maybe_signed_header else {
                debug!("Header query sent to network finished");
                return Ok(P2PSyncControl::QueryFinishedPartially);
            };
            // TODO(shahak): Check that parent_hash is the same as the previous block's hash
            // and handle reverts.
            if *current_block_number != block_header.block_number {
                return Err(P2PSyncError::HeadersUnordered {
                    expected_block_number: *current_block_number,
                    actual_block_number: block_header.block_number,
                });
            }
            if signatures.len() != ALLOWED_SIGNATURES_LENGTH {
                return Err(P2PSyncError::WrongSignaturesLength { signatures });
            }
            self.storage_writer
                .begin_rw_txn()?
                .append_header(*current_block_number, &block_header)?
                .append_block_signature(
                    *current_block_number,
                    signatures.first().expect(
                        "Calling first on a vector of size {ALLOWED_SIGNATURES_LENGTH} returned \
                         None",
                    ),
                )?
                .commit()?;
            info!("Added block {}.", current_block_number);
            *current_block_number = current_block_number.unchecked_next();
        }
        // Consume the None message signaling the end of the query.
        match self.response_receivers.signed_headers_receiver.next().await {
            Some(None) => Ok(P2PSyncControl::ContinueDownloading),
            Some(Some(_)) => Err(P2PSyncError::TooManyResponses),
            None => Err(P2PSyncError::ReceiverChannelTerminated),
        }
    }
}
