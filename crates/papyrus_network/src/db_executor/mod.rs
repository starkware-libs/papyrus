use std::vec;

use async_trait::async_trait;
use futures::future::pending;
use futures::{FutureExt, Sink, SinkExt, StreamExt};
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{db, StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;
use tracing::error;

use crate::network_manager::SqmrQueryReceiver;

#[cfg(test)]
mod test;

mod utils;

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
    fn set_header_queries_receiver(
        &mut self,
        receiver: SqmrQueryReceiver<HeaderQuery, DataOrFin<SignedBlockHeader>>,
    );

    fn set_state_diff_queries_receiver(
        &mut self,
        receiver: SqmrQueryReceiver<StateDiffQuery, DataOrFin<StateDiffChunk>>,
    );

    /// Polls incoming queries.
    // TODO(shahak): Consume self.
    async fn run(&mut self);
}

pub struct DBExecutor {
    storage_reader: StorageReader,
    // TODO(shahak): Make this non-option.
    header_queries_receiver: Option<SqmrQueryReceiver<HeaderQuery, DataOrFin<SignedBlockHeader>>>,
    // TODO(shahak): Make this non-option.
    state_diff_queries_receiver:
        Option<SqmrQueryReceiver<StateDiffQuery, DataOrFin<StateDiffChunk>>>,
}

#[async_trait]
impl DBExecutorTrait for DBExecutor {
    fn set_header_queries_receiver(
        &mut self,
        receiver: SqmrQueryReceiver<HeaderQuery, DataOrFin<SignedBlockHeader>>,
    ) {
        self.header_queries_receiver = Some(receiver);
    }

    fn set_state_diff_queries_receiver(
        &mut self,
        receiver: SqmrQueryReceiver<StateDiffQuery, DataOrFin<StateDiffChunk>>,
    ) {
        self.state_diff_queries_receiver = Some(receiver);
    }

    async fn run(&mut self) {
        loop {
            let header_queries_receiver_future =
                if let Some(header_queries_receiver) = self.header_queries_receiver.as_mut() {
                    header_queries_receiver.next().boxed()
                } else {
                    pending().boxed()
                };
            let state_diff_queries_receiver_future = if let Some(state_diff_queries_receiver) =
                self.state_diff_queries_receiver.as_mut()
            {
                state_diff_queries_receiver.next().boxed()
            } else {
                pending().boxed()
            };

            tokio::select! {
                result = header_queries_receiver_future => {
                    let (query_result, response_sender) = result.expect(
                        "Header queries sender was unexpectedly dropped."
                    );
                // TODO(shahak): Report if query_result is Err.
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
                result = state_diff_queries_receiver_future => {
                    let (query_result, response_sender) = result.expect(
                        "State diff queries sender was unexpectedly dropped."
                    );
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
            };
        }
    }
}

impl DBExecutor {
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { storage_reader, header_queries_receiver: None, state_diff_queries_receiver: None }
    }

    fn register_query<Data, Sender>(&self, query: Query, sender: Sender)
    where
        Data: FetchBlockDataFromDb + Send + 'static,
        Sender: Sink<DataOrFin<Data>> + Unpin + Send + 'static,
        DBExecutorError: From<<Sender as Sink<DataOrFin<Data>>>::Error>,
    {
        let storage_reader_clone = self.storage_reader.clone();
        tokio::task::spawn(async move {
            let result = send_data_for_query(storage_reader_clone, query.clone(), sender).await;
            if let Err(error) = result {
                if error.should_log_in_error_level() {
                    error!("Running inbound query {query:?} failed on {error:?}");
                }
                Err(error)
            } else {
                Ok(())
            }
        });
    }
}

pub trait FetchBlockDataFromDb: Sized {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, DBExecutorError>;
}

impl FetchBlockDataFromDb for SignedBlockHeader {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, DBExecutorError> {
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
        Ok(vec![SignedBlockHeader { block_header: header, signatures: vec![signature] }])
    }
}

impl FetchBlockDataFromDb for StateDiffChunk {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, DBExecutorError> {
        let thin_state_diff =
            txn.get_state_diff(block_number)?.ok_or(DBExecutorError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        Ok(split_thin_state_diff(thin_state_diff))
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

async fn send_data_for_query<Data, Sender>(
    storage_reader: StorageReader,
    query: Query,
    mut sender: Sender,
) -> Result<(), DBExecutorError>
where
    Data: FetchBlockDataFromDb + Send + 'static,
    Sender: Sink<DataOrFin<Data>> + Unpin + Send + 'static,
    DBExecutorError: From<<Sender as Sink<DataOrFin<Data>>>::Error>,
{
    // If this function fails, we still want to send fin before failing.
    let result = send_data_without_fin_for_query(&storage_reader, query, &mut sender).await;
    sender.feed(DataOrFin(None)).await?;
    result
}

async fn send_data_without_fin_for_query<Data, Sender>(
    storage_reader: &StorageReader,
    query: Query,
    sender: &mut Sender,
) -> Result<(), DBExecutorError>
where
    Data: FetchBlockDataFromDb + Send + 'static,
    Sender: Sink<DataOrFin<Data>> + Unpin + Send + 'static,
    DBExecutorError: From<<Sender as Sink<DataOrFin<Data>>>::Error>,
{
    let txn = storage_reader.begin_ro_txn()?;
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
        let block_number =
            BlockNumber(utils::calculate_block_number(&query, start_block_number, block_counter)?);
        let data_vec = Data::fetch_block_data_from_db(block_number, &txn)?;
        for data in data_vec {
            // TODO: consider implement retry mechanism.
            sender.feed(DataOrFin(Some(data))).await?;
        }
    }
    Ok(())
}
