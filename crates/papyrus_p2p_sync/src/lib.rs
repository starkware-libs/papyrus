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
use starknet_api::block::BlockNumber;
use tokio::time::timeout;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    pub header_query_size: usize,
    // TODO(shahak): Remove timeout and check if query finished when the network reports it.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub query_timeout: Duration,
}

impl SerializeConfig for SyncConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "header_query_size",
                &self.header_query_size,
                "The size of each header query.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "query_timeout",
                &self.query_timeout.as_secs(),
                "Time in seconds to wait for query responses until we restart the query from the \
                 last received block.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig { header_query_size: 100, query_timeout: Duration::from_secs(5) }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum P2PSyncError {
    #[error(
        "Blocks returned unordered from the network. Expected header with \
         {expected_block_number}, got {actual_block_number}."
    )]
    HeadersUnordered { expected_block_number: BlockNumber, actual_block_number: BlockNumber },
    #[error(
        "Expected to receive one signature from the network. got {signatures_len} signatures \
         instead."
    )]
    WrongSignaturesLength { signatures_len: usize },
    #[error("Network has closed its channel for sending responses.")]
    ReceiverChannelTerminated,
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    SendError(#[from] SendError),
}
pub struct P2PSync {
    config: SyncConfig,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    query_sender: Sender<Query>,
    response_receivers: ResponseReceivers,
}

impl P2PSync {
    pub fn new(
        config: SyncConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        query_sender: Sender<Query>,
        response_receivers: ResponseReceivers,
    ) -> Self {
        Self { config, storage_reader, storage_writer, query_sender, response_receivers }
    }

    pub async fn run(mut self) -> Result<(), P2PSyncError> {
        let mut current_block_number = self.storage_reader.begin_ro_txn()?.get_header_marker()?;
        loop {
            self.query_sender
                .send(Query {
                    start_block: current_block_number,
                    direction: Direction::Forward,
                    limit: self.config.header_query_size,
                    step: 1,
                    data_type: DataType::SignedBlockHeader,
                })
                .await?;
            let end_block_number = current_block_number.0
                + u64::try_from(self.config.header_query_size)
                    .expect("Failed converting usize to u64");
            while current_block_number.0 < end_block_number {
                let Ok(maybe_signed_header) = timeout(
                    self.config.query_timeout,
                    self.response_receivers.signed_headers_receiver.next(),
                )
                .await
                else {
                    break;
                };
                let Some(SignedBlockHeader { block_header, signatures }) = maybe_signed_header
                else {
                    return Err(P2PSyncError::ReceiverChannelTerminated);
                };
                if current_block_number != block_header.block_number {
                    return Err(P2PSyncError::HeadersUnordered {
                        expected_block_number: current_block_number,
                        actual_block_number: block_header.block_number,
                    });
                }
                if signatures.len() != 1 {
                    return Err(P2PSyncError::WrongSignaturesLength {
                        signatures_len: signatures.len(),
                    });
                }
                self.storage_writer
                    .begin_rw_txn()?
                    .append_header(current_block_number, &block_header)?
                    .append_block_signature(
                        current_block_number,
                        signatures
                            .first()
                            .expect("Calling first on a vector of size 1 returned None"),
                    )?
                    .commit()?;
                current_block_number = current_block_number.next();
            }
        }
    }
}
