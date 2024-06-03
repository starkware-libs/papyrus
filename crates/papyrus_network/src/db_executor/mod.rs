use std::pin::Pin;
use std::task::Poll;
use std::vec;

use bytes::BufMut;
use derive_more::Display;
use futures::channel::mpsc::Sender;
use futures::future::poll_fn;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
#[cfg(test)]
use mockall::automock;
use papyrus_protobuf::converters::common::volition_domain_to_enum_int;
use papyrus_protobuf::converters::state_diff::DOMAIN;
use papyrus_protobuf::protobuf;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffChunkVec,
};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{db, StorageReader, StorageTxn};
use prost::Message;
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;
use tokio::task::JoinHandle;

use crate::DataType;

#[cfg(test)]
mod test;

mod utils;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Display)]
pub struct QueryId(pub usize);

#[derive(thiserror::Error, Debug)]
#[error("Failed to encode data")]
pub struct DataEncodingError;

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone)]
pub enum Data {
    BlockHeaderAndSignature(SignedBlockHeader),
    StateDiffChunk { state_diff: StateDiffChunk },
    Fin(DataType),
}

impl Default for Data {
    fn default() -> Self {
        // TODO: consider this default data type.
        Data::Fin(DataType::SignedBlockHeader)
    }
}

impl Data {
    fn encode_template<B>(
        self,
        buf: &mut B,
        encode_with_length_prefix_flag: bool,
    ) -> Result<(), DataEncodingError>
    where
        B: BufMut,
    {
        match self {
            Data::BlockHeaderAndSignature(signed_block_header) => {
                let data: protobuf::BlockHeadersResponse = Some(signed_block_header).into();
                match encode_with_length_prefix_flag {
                    true => data.encode_length_delimited(buf).map_err(|_| DataEncodingError),
                    false => data.encode(buf).map_err(|_| DataEncodingError),
                }
            }
            Data::StateDiffChunk { state_diff } => {
                let x = DataOrFin(Some(state_diff));
                let state_diffs_response = protobuf::StateDiffsResponse::from(x);
                match encode_with_length_prefix_flag {
                    true => state_diffs_response.encode_length_delimited(buf),
                    false => state_diffs_response.encode(buf),
                }
                .map_err(|_| DataEncodingError)
            }
            Data::Fin(data_type) => match data_type {
                DataType::SignedBlockHeader => {
                    let block_header_response = protobuf::BlockHeadersResponse {
                        header_message: Some(protobuf::block_headers_response::HeaderMessage::Fin(
                            protobuf::Fin {},
                        )),
                    };
                    match encode_with_length_prefix_flag {
                        true => block_header_response.encode_length_delimited(buf),
                        false => block_header_response.encode(buf),
                    }
                    .map_err(|_| DataEncodingError)
                }
                DataType::StateDiff => {
                    let state_diff_response = protobuf::StateDiffsResponse {
                        state_diff_message: Some(
                            protobuf::state_diffs_response::StateDiffMessage::Fin(protobuf::Fin {}),
                        ),
                    };
                    match encode_with_length_prefix_flag {
                        true => state_diff_response.encode_length_delimited(buf),
                        false => state_diff_response.encode(buf),
                    }
                    .map_err(|_| DataEncodingError)
                }
            },
        }
    }
    pub fn encode_with_length_prefix<B>(self, buf: &mut B) -> Result<(), DataEncodingError>
    where
        B: BufMut,
    {
        self.encode_template(buf, true)
    }
    pub fn encode_without_length_prefix<B>(self, buf: &mut B) -> Result<(), DataEncodingError>
    where
        B: BufMut,
    {
        self.encode_template(buf, false)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DBExecutorError {
    #[error("Storage error. Query id: {query_id}, error: {storage_error:?}")]
    DBInternalError {
        query_id: QueryId,
        #[source]
        storage_error: papyrus_storage::StorageError,
    },
    #[error(
        "Block number is out of range. Query: {query:?}, counter: {counter}, query_id: {query_id}"
    )]
    BlockNumberOutOfRange { query: Query, counter: u64, query_id: QueryId },
    // TODO: add data type to the error message.
    #[error("Block not found. Block: {block_hash_or_number:?}, query_id: {query_id}")]
    BlockNotFound { block_hash_or_number: BlockHashOrNumber, query_id: QueryId },
    // This error should be non recoverable.
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    // TODO: remove this error, use BlockNotFound instead.
    // This error should be non recoverable.
    #[error(
        "Block {block_number:?} is in the storage but its signature isn't. query_id: {query_id}"
    )]
    SignatureNotFound { block_number: BlockNumber, query_id: QueryId },
    #[error("Send error. Query id: {query_id}, error: {send_error:?}")]
    SendError {
        query_id: QueryId,
        #[source]
        send_error: futures::channel::mpsc::SendError,
    },
}

impl DBExecutorError {
    pub fn query_id(&self) -> Option<QueryId> {
        match self {
            Self::DBInternalError { query_id, .. }
            | Self::BlockNumberOutOfRange { query_id, .. }
            | Self::BlockNotFound { query_id, .. }
            | Self::SignatureNotFound { query_id, .. }
            | Self::SendError { query_id, .. } => Some(*query_id),
            Self::JoinError(_) => None,
        }
    }

    pub fn should_log_in_error_level(&self) -> bool {
        match self {
            Self::JoinError(_) | Self::SignatureNotFound { .. } | Self::SendError { .. }
            // TODO(shahak): Consider returning false for some of the StorageError variants.
            | Self::DBInternalError { .. } => true,
            Self::BlockNumberOutOfRange { .. } | Self::BlockNotFound { .. } => false,
        }
    }
}

/// DBExecutorTrait is a stream of queries. Each result is marks the end of a query fulfillment.
/// A query can either succeed (and return Ok(QueryId)) or fail (and return Err(DBExecutorError)).
/// The stream is never exhausted, and it is the responsibility of the user to poll it.
pub trait DBExecutorTrait: Stream<Item = Result<QueryId, DBExecutorError>> + Unpin {
    // TODO: add writer functionality
    fn register_query(
        &mut self,
        query: Query,
        data_type: impl FetchBlockDataFromDb + Send + 'static,
        sender: Sender<Vec<Data>>,
    ) -> QueryId;
}

// TODO: currently this executor returns only block headers and signatures.
pub struct DBExecutor {
    next_query_id: usize,
    storage_reader: StorageReader,
    query_execution_set: FuturesUnordered<JoinHandle<Result<QueryId, DBExecutorError>>>,
}

impl DBExecutor {
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { next_query_id: 0, storage_reader, query_execution_set: FuturesUnordered::new() }
    }
}

impl DBExecutorTrait for DBExecutor {
    fn register_query(
        &mut self,
        query: Query,
        data_type: impl FetchBlockDataFromDb + Send + 'static,
        mut sender: Sender<Vec<Data>>,
    ) -> QueryId {
        let query_id = QueryId(self.next_query_id);
        self.next_query_id += 1;
        let storage_reader_clone = self.storage_reader.clone();
        self.query_execution_set.push(tokio::task::spawn(async move {
            {
                let txn = storage_reader_clone.begin_ro_txn().map_err(|err| {
                    DBExecutorError::DBInternalError { query_id, storage_error: err }
                })?;
                let start_block_number = match query.start_block {
                    BlockHashOrNumber::Number(BlockNumber(num)) => num,
                    BlockHashOrNumber::Hash(block_hash) => {
                        txn.get_block_number_by_hash(&block_hash)
                            .map_err(|err| DBExecutorError::DBInternalError {
                                query_id,
                                storage_error: err,
                            })?
                            .ok_or(DBExecutorError::BlockNotFound {
                                block_hash_or_number: BlockHashOrNumber::Hash(block_hash),
                                query_id,
                            })?
                            .0
                    }
                };
                for block_counter in 0..query.limit {
                    let block_number = BlockNumber(utils::calculate_block_number(
                        &query,
                        start_block_number,
                        block_counter,
                        query_id,
                    )?);
                    let data_vec =
                        data_type.fetch_block_data_from_db(block_number, query_id, &txn)?;
                    // Using poll_fn because Sender::poll_ready is not a future
                    match poll_fn(|cx| sender.poll_ready(cx)).await {
                        Ok(()) => {
                            if let Err(e) = sender.start_send(data_vec) {
                                // TODO: consider implement retry mechanism.
                                return Err(DBExecutorError::SendError { query_id, send_error: e });
                            };
                        }
                        Err(e) => {
                            return Err(DBExecutorError::SendError { query_id, send_error: e });
                        }
                    }
                }
                Ok(query_id)
            }
        }));
        query_id
    }
}

impl Stream for DBExecutor {
    type Item = Result<QueryId, DBExecutorError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        poll_query_execution_set(&mut Pin::into_inner(self).query_execution_set, cx)
    }
}

pub(crate) fn poll_query_execution_set(
    query_execution_set: &mut FuturesUnordered<JoinHandle<Result<QueryId, DBExecutorError>>>,
    cx: &mut std::task::Context<'_>,
) -> Poll<Option<Result<QueryId, DBExecutorError>>> {
    match query_execution_set.poll_next_unpin(cx) {
        Poll::Ready(Some(join_result)) => {
            let res = join_result?;
            Poll::Ready(Some(res))
        }
        Poll::Ready(None) => {
            *query_execution_set = FuturesUnordered::new();
            Poll::Pending
        }
        Poll::Pending => Poll::Pending,
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
        query_id: QueryId,
        txn: &StorageTxn<'a, db::RO>,
    ) -> Result<Vec<Data>, DBExecutorError>;
}

impl FetchBlockDataFromDb for DataType {
    fn fetch_block_data_from_db(
        &self,
        block_number: BlockNumber,
        query_id: QueryId,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Data>, DBExecutorError> {
        match self {
            DataType::SignedBlockHeader => {
                let mut header = txn
                    .get_block_header(block_number)
                    .map_err(|err| DBExecutorError::DBInternalError {
                        query_id,
                        storage_error: err,
                    })?
                    .ok_or(DBExecutorError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                        query_id,
                    })?;
                // TODO(shahak) Remove this once central sync fills the state_diff_length field.
                if header.state_diff_length.is_none() {
                    header.state_diff_length = Some(
                        txn.get_state_diff(block_number)
                            .map_err(|err| DBExecutorError::DBInternalError {
                                query_id,
                                storage_error: err,
                            })?
                            .ok_or(DBExecutorError::BlockNotFound {
                                block_hash_or_number: BlockHashOrNumber::Number(block_number),
                                query_id,
                            })?
                            .len(),
                    );
                }
                let signature = txn
                    .get_block_signature(block_number)
                    .map_err(|err| DBExecutorError::DBInternalError {
                        query_id,
                        storage_error: err,
                    })?
                    .ok_or(DBExecutorError::SignatureNotFound { block_number, query_id })?;
                Ok(vec![Data::BlockHeaderAndSignature(SignedBlockHeader {
                    block_header: header,
                    signatures: vec![signature],
                })])
            }
            DataType::StateDiff => {
                let thin_state_diff = txn
                    .get_state_diff(block_number)
                    .map_err(|err| DBExecutorError::DBInternalError {
                        query_id,
                        storage_error: err,
                    })?
                    .ok_or(DBExecutorError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                        query_id,
                    })?;
                let vec_data = StateDiffChunkVec::from(thin_state_diff)
                    .0
                    .into_iter()
                    .map(|state_diff| Data::StateDiffChunk { state_diff })
                    .collect();
                Ok(vec_data)
            }
        }
    }
}

// A wrapper struct for Vec<StateDiffsResponse> so that we can implement traits for it.
pub struct StateDiffsResponseVec(pub Vec<protobuf::StateDiffsResponse>);

impl From<ThinStateDiff> for StateDiffsResponseVec {
    fn from(value: ThinStateDiff) -> Self {
        let mut result = Vec::new();

        for (contract_address, class_hash) in
            value.deployed_contracts.into_iter().chain(value.replaced_classes.into_iter())
        {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::ContractDiff(
                        protobuf::ContractDiff {
                            address: Some(contract_address.into()),
                            class_hash: Some(class_hash.0.into()),
                            domain: volition_domain_to_enum_int(DOMAIN),
                            ..Default::default()
                        },
                    ),
                ),
            });
        }
        for (contract_address, storage_diffs) in value.storage_diffs {
            if storage_diffs.is_empty() {
                continue;
            }
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::ContractDiff(
                        protobuf::ContractDiff {
                            address: Some(contract_address.into()),
                            values: storage_diffs
                                .into_iter()
                                .map(|(key, value)| protobuf::ContractStoredValue {
                                    key: Some((*key.0.key()).into()),
                                    value: Some(value.into()),
                                })
                                .collect(),
                            domain: volition_domain_to_enum_int(DOMAIN),
                            ..Default::default()
                        },
                    ),
                ),
            });
        }
        for (contract_address, nonce) in value.nonces {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::ContractDiff(
                        protobuf::ContractDiff {
                            address: Some(contract_address.into()),
                            nonce: Some(nonce.0.into()),
                            domain: volition_domain_to_enum_int(DOMAIN),
                            ..Default::default()
                        },
                    ),
                ),
            });
        }

        for (class_hash, compiled_class_hash) in value.declared_classes {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                        protobuf::DeclaredClass {
                            class_hash: Some(class_hash.0.into()),
                            compiled_class_hash: Some(compiled_class_hash.0.into()),
                        },
                    ),
                ),
            });
        }
        for class_hash in value.deprecated_declared_classes {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                        protobuf::DeclaredClass {
                            class_hash: Some(class_hash.0.into()),
                            compiled_class_hash: None,
                        },
                    ),
                ),
            });
        }

        Self(result)
    }
}
