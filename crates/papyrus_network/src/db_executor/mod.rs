use std::vec;

use async_trait::async_trait;
use bytes::BufMut;
use futures::channel::mpsc::Sender;
use futures::future::{pending, poll_fn};
#[cfg(test)]
use mockall::automock;
use papyrus_protobuf::protobuf;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{db, StorageReader, StorageTxn};
use prost::Message;
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;
use tracing::error;

use crate::DataType;

#[cfg(test)]
mod test;

mod utils;

#[derive(thiserror::Error, Debug)]
#[error("Failed to encode data")]
pub struct DataEncodingError;

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone)]
pub enum Data {
    BlockHeaderAndSignature(SignedBlockHeader),
    StateDiffChunk(StateDiffChunk),
    Fin(DataType),
}

impl Default for Data {
    fn default() -> Self {
        // TODO: consider this default data type.
        Data::Fin(DataType::SignedBlockHeader)
    }
}

impl Data {
    pub fn encode<B>(self, buf: &mut B) -> Result<(), DataEncodingError>
    where
        B: BufMut,
    {
        match self {
            Data::BlockHeaderAndSignature(signed_block_header) => {
                let data: protobuf::BlockHeadersResponse = Some(signed_block_header).into();
                data.encode(buf).map_err(|_| DataEncodingError)
            }
            Data::StateDiffChunk(state_diff) => {
                let state_diff_chunk = DataOrFin(Some(state_diff));
                let state_diffs_response = protobuf::StateDiffsResponse::from(state_diff_chunk);
                state_diffs_response.encode(buf).map_err(|_| DataEncodingError)
            }
            Data::Fin(data_type) => match data_type {
                DataType::SignedBlockHeader => {
                    let block_header_response = protobuf::BlockHeadersResponse {
                        header_message: Some(protobuf::block_headers_response::HeaderMessage::Fin(
                            protobuf::Fin {},
                        )),
                    };
                    block_header_response.encode(buf).map_err(|_| DataEncodingError)
                }
                DataType::StateDiff => {
                    let state_diff_response = protobuf::StateDiffsResponse {
                        state_diff_message: Some(
                            protobuf::state_diffs_response::StateDiffMessage::Fin(protobuf::Fin {}),
                        ),
                    };
                    state_diff_response.encode(buf).map_err(|_| DataEncodingError)
                }
            },
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DBExecutorError {
    #[error(transparent)]
    DBInternalError(#[from] papyrus_storage::StorageError),
    #[error("Block number is out of range. Query: {query:?}, counter: {counter}")]
    BlockNumberOutOfRange { query: Query, counter: u64 },
    // TODO: add data type to the error message.
    #[error("Block not found. Block: {block_hash_or_number:?}")]
    BlockNotFound { block_hash_or_number: BlockHashOrNumber },
    // This error should be non recoverable.
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    // TODO: remove this error, use BlockNotFound instead.
    // This error should be non recoverable.
    #[error("Block {block_number:?} is in the storage but its signature isn't.")]
    SignatureNotFound { block_number: BlockNumber },
    #[error(transparent)]
    SendError(#[from] futures::channel::mpsc::SendError),
}

impl DBExecutorError {
    pub fn should_log_in_error_level(&self) -> bool {
        match self {
            Self::JoinError(_) | Self::SignatureNotFound { .. } | Self::SendError { .. }
            // TODO(shahak): Consider returning false for some of the StorageError variants.
            | Self::DBInternalError { .. } => true,
            Self::BlockNumberOutOfRange { .. } | Self::BlockNotFound { .. } => false,
        }
    }
}

/// A DBExecutor receives inbound queries and returns their corresponding data.
#[async_trait]
pub trait DBExecutorTrait {
    /// Send a query to be executed in the DBExecutor. The query will be run concurrently with the
    /// calling code and the result will be over the given channel.
    fn register_query(
        &mut self,
        query: Query,
        data_type: impl FetchBlockDataFromDb + Send + 'static,
        sender: Sender<Data>,
    );

    /// Polls incoming queries.
    // TODO(shahak): Consume self.
    async fn run(&mut self);
}

pub struct DBExecutor {
    storage_reader: StorageReader,
}

impl DBExecutor {
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { storage_reader }
    }
}

#[async_trait]
impl DBExecutorTrait for DBExecutor {
    fn register_query(
        &mut self,
        query: Query,
        data_type: impl FetchBlockDataFromDb + Send + 'static,
        mut sender: Sender<Data>,
    ) {
        let storage_reader_clone = self.storage_reader.clone();
        tokio::task::spawn(async move {
            let result: Result<(), DBExecutorError> = {
                let txn = storage_reader_clone.begin_ro_txn()?;
                let start_block_number = match query.start_block {
                    BlockHashOrNumber::Number(BlockNumber(num)) => num,
                    BlockHashOrNumber::Hash(block_hash) => {
                        txn.get_block_number_by_hash(&block_hash)?
                            .ok_or(DBExecutorError::BlockNotFound {
                                block_hash_or_number: BlockHashOrNumber::Hash(block_hash),
                            })?
                            .0
                    }
                };
                for block_counter in 0..query.limit {
                    let block_number = BlockNumber(utils::calculate_block_number(
                        &query,
                        start_block_number,
                        block_counter,
                    )?);
                    let data_vec = data_type.fetch_block_data_from_db(block_number, &txn)?;
                    // Using poll_fn because Sender::poll_ready is not a future
                    poll_fn(|cx| sender.poll_ready(cx)).await?;
                    for data in data_vec {
                        // TODO: consider implement retry mechanism.
                        sender.start_send(data)?;
                    }
                }
                Ok(())
            };
            if let Err(error) = &result {
                if error.should_log_in_error_level() {
                    error!("Running inbound query {query:?} failed on {error:?}");
                }
            }
            result
        });
    }

    async fn run(&mut self) {
        // TODO(shahak): Parse incoming queries once we receive them through channel instead of
        // through function.
        pending::<()>().await
    }
}

#[cfg_attr(test, automock)]
// we need to tell clippy to ignore the "needless" lifetime warning because it's not true.
// we do need the lifetime for the automock, following clippy's suggestion will break the code.
#[allow(clippy::needless_lifetimes)]
pub trait FetchBlockDataFromDb {
    fn fetch_block_data_from_db<'a>(
        &self,
        block_number: BlockNumber,
        txn: &StorageTxn<'a, db::RO>,
    ) -> Result<Vec<Data>, DBExecutorError>;
}

impl FetchBlockDataFromDb for DataType {
    fn fetch_block_data_from_db(
        &self,
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Data>, DBExecutorError> {
        match self {
            DataType::SignedBlockHeader => {
                let mut header =
                    txn.get_block_header(block_number)?.ok_or(DBExecutorError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                    })?;
                // TODO(shahak) Remove this once central sync fills the state_diff_length field.
                if header.state_diff_length.is_none() {
                    header.state_diff_length = Some(
                        txn.get_state_diff(block_number)?
                            .ok_or(DBExecutorError::BlockNotFound {
                                block_hash_or_number: BlockHashOrNumber::Number(block_number),
                            })?
                            .len(),
                    );
                }
                let signature = txn
                    .get_block_signature(block_number)?
                    .ok_or(DBExecutorError::SignatureNotFound { block_number })?;
                Ok(vec![Data::BlockHeaderAndSignature(SignedBlockHeader {
                    block_header: header,
                    signatures: vec![signature],
                })])
            }
            DataType::StateDiff => {
                let thin_state_diff =
                    txn.get_state_diff(block_number)?.ok_or(DBExecutorError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                    })?;
                let vec_data = split_thin_state_diff(thin_state_diff)
                    .into_iter()
                    .map(Data::StateDiffChunk)
                    .collect();
                Ok(vec_data)
            }
        }
    }
}

pub fn split_thin_state_diff(thin_state_diff: ThinStateDiff) -> Vec<StateDiffChunk> {
    let mut state_diff_chunks = Vec::new();
    let mut contract_addresses = std::collections::HashSet::new();

    contract_addresses.extend(
        thin_state_diff
            .deployed_contracts
            .keys()
            .chain(thin_state_diff.replaced_classes.keys())
            .chain(thin_state_diff.nonces.keys())
            .chain(thin_state_diff.storage_diffs.keys()),
    );
    for contract_address in contract_addresses {
        let class_hash = thin_state_diff
            .deployed_contracts
            .get(&contract_address)
            .or_else(|| thin_state_diff.replaced_classes.get(&contract_address))
            .cloned();
        let storage_diffs =
            thin_state_diff.storage_diffs.get(&contract_address).cloned().unwrap_or_default();
        let nonce = thin_state_diff.nonces.get(&contract_address).cloned();
        state_diff_chunks.push(StateDiffChunk::ContractDiff(ContractDiff {
            contract_address,
            class_hash,
            nonce,
            storage_diffs,
        }));
    }

    for (class_hash, compiled_class_hash) in thin_state_diff.declared_classes {
        state_diff_chunks
            .push(StateDiffChunk::DeclaredClass(DeclaredClass { class_hash, compiled_class_hash }));
    }

    for class_hash in thin_state_diff.deprecated_declared_classes {
        state_diff_chunks
            .push(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass { class_hash }));
    }
    state_diff_chunks
}
