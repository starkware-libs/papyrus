use std::vec;

use futures::channel::mpsc::SendError;
use futures::{Sink, SinkExt, Stream, StreamExt};
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ClassQuery,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    EventQuery,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::class::ClassStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{db, StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::{Event, Transaction, TransactionHash, TransactionOutput};
use tracing::error;

#[cfg(test)]
mod test;

mod utils;

#[derive(thiserror::Error, Debug)]
pub enum P2PSyncServerError {
    #[error(transparent)]
    DBInternalError(#[from] papyrus_storage::StorageError),
    #[error("Block number is out of range. Query: {query:?}, counter: {counter}")]
    BlockNumberOutOfRange { query: Query, counter: u64 },
    // TODO: add data type to the error message.
    #[error("Block not found. Block: {block_hash_or_number:?}")]
    BlockNotFound { block_hash_or_number: BlockHashOrNumber },
    #[error("Class not found. Class hash: {class_hash}")]
    ClassNotFound { class_hash: ClassHash },
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

impl P2PSyncServerError {
    pub fn should_log_in_error_level(&self) -> bool {
        match self {
            Self::JoinError(_) | Self::SignatureNotFound { .. } | Self::SendError { .. }
            // TODO(shahak): Consider returning false for some of the StorageError variants.
            | Self::DBInternalError { .. } => true,
            Self::BlockNumberOutOfRange { .. } | Self::BlockNotFound { .. } | Self::ClassNotFound { .. } => false,
        }
    }
}

/// A P2PSyncServer receives inbound queries and returns their corresponding data.
pub struct P2PSyncServer<
    HeaderQueryReceiver,
    StateDiffQueryReceiver,
    TransactionQueryReceiver,
    ClassQueryReceiver,
    EventQueryReceiver,
> {
    storage_reader: StorageReader,
    header_queries_receiver: HeaderQueryReceiver,
    state_diff_queries_receiver: StateDiffQueryReceiver,
    transaction_queries_receiver: TransactionQueryReceiver,
    class_queries_receiver: ClassQueryReceiver,
    event_queries_receiver: EventQueryReceiver,
}

impl<
    HeaderQueryReceiver,
    StateDiffQueryReceiver,
    TransactionQueryReceiver,
    ClassQueryReceiver,
    EventQueryReceiver,
    HeaderResponsesSender,
    StateDiffResponsesSender,
    TransactionResponsesSender,
    ClassResponsesSender,
    EventResponsesSender,
>
    P2PSyncServer<
        HeaderQueryReceiver,
        StateDiffQueryReceiver,
        TransactionQueryReceiver,
        ClassQueryReceiver,
        EventQueryReceiver,
    >
where
    HeaderQueryReceiver: Stream<Item = (Result<HeaderQuery, ProtobufConversionError>, HeaderResponsesSender)>
        + Unpin,
    HeaderResponsesSender:
        Sink<DataOrFin<SignedBlockHeader>, Error = SendError> + Unpin + Send + 'static,
    StateDiffQueryReceiver: Stream<Item = (Result<StateDiffQuery, ProtobufConversionError>, StateDiffResponsesSender)>
        + Unpin,
    StateDiffResponsesSender:
        Sink<DataOrFin<StateDiffChunk>, Error = SendError> + Unpin + Send + 'static,
    TransactionQueryReceiver: Stream<
            Item = (Result<TransactionQuery, ProtobufConversionError>, TransactionResponsesSender),
        > + Unpin,
    TransactionResponsesSender: Sink<DataOrFin<(Transaction, TransactionOutput)>, Error = SendError>
        + Unpin
        + Send
        + 'static,
    ClassQueryReceiver:
        Stream<Item = (Result<ClassQuery, ProtobufConversionError>, ClassResponsesSender)> + Unpin,
    ClassResponsesSender:
        Sink<DataOrFin<ApiContractClass>, Error = SendError> + Unpin + Send + 'static,
    EventQueryReceiver:
        Stream<Item = (Result<EventQuery, ProtobufConversionError>, EventResponsesSender)> + Unpin,
    EventResponsesSender:
        Sink<DataOrFin<(Event, TransactionHash)>, Error = SendError> + Unpin + Send + 'static,
{
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                result = self.header_queries_receiver.next() => {
                    let (query_result, response_sender) = result.expect(
                        "Header queries sender was unexpectedly dropped."
                    );
                    // TODO(shahak): Report if query_result is Err.
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
                result = self.state_diff_queries_receiver.next() => {
                    let (query_result, response_sender) = result.expect(
                        "State diff queries sender was unexpectedly dropped."
                    );
                    // TODO(shahak): Report if query_result is Err.
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
                result = self.transaction_queries_receiver.next() => {
                    let (query_result, response_sender) = result.expect(
                        "Transaction queries sender was unexpectedly dropped."
                    );
                    // TODO: Report if query_result is Err.
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
                result = self.class_queries_receiver.next() => {
                    let (query_result, response_sender) = result.expect(
                        "Class queries sender was unexpectedly dropped."
                    );
                    // TODO: Report if query_result is Err.
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
                result = self.event_queries_receiver.next() => {
                    let (query_result, response_sender) = result.expect(
                        "Event queries sender was unexpectedly dropped."
                    );
                    // TODO: Report if query_result is Err.
                    if let Ok(query) = query_result {
                        self.register_query(query.0, response_sender);
                    }
                }
            };
        }
    }

    pub fn new(
        storage_reader: StorageReader,
        header_queries_receiver: HeaderQueryReceiver,
        state_diff_queries_receiver: StateDiffQueryReceiver,
        transaction_queries_receiver: TransactionQueryReceiver,
        class_queries_receiver: ClassQueryReceiver,
        event_queries_receiver: EventQueryReceiver,
    ) -> Self {
        Self {
            storage_reader,
            header_queries_receiver,
            state_diff_queries_receiver,
            transaction_queries_receiver,
            class_queries_receiver,
            event_queries_receiver,
        }
    }

    fn register_query<Data, Sender>(&self, query: Query, sender: Sender)
    where
        Data: FetchBlockDataFromDb + Send + 'static,
        Sender: Sink<DataOrFin<Data>> + Unpin + Send + 'static,
        P2PSyncServerError: From<<Sender as Sink<DataOrFin<Data>>>::Error>,
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
    ) -> Result<Vec<Self>, P2PSyncServerError>;
}

impl FetchBlockDataFromDb for SignedBlockHeader {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, P2PSyncServerError> {
        let mut header =
            txn.get_block_header(block_number)?.ok_or(P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        // TODO(shahak) Remove this once central sync fills the state_diff_length field.
        if header.state_diff_length.is_none() {
            header.state_diff_length = Some(
                txn.get_state_diff(block_number)?
                    .ok_or(P2PSyncServerError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                    })?
                    .len(),
            );
        }
        let signature = txn
            .get_block_signature(block_number)?
            .ok_or(P2PSyncServerError::SignatureNotFound { block_number })?;
        Ok(vec![SignedBlockHeader { block_header: header, signatures: vec![signature] }])
    }
}

impl FetchBlockDataFromDb for StateDiffChunk {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, P2PSyncServerError> {
        let thin_state_diff =
            txn.get_state_diff(block_number)?.ok_or(P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        Ok(split_thin_state_diff(thin_state_diff))
    }
}

impl FetchBlockDataFromDb for (Transaction, TransactionOutput) {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, P2PSyncServerError> {
        let transactions =
            txn.get_block_transactions(block_number)?.ok_or(P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        let transaction_outputs = txn.get_block_transaction_outputs(block_number)?.ok_or(
            P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;
        let mut result: Vec<(Transaction, TransactionOutput)> = Vec::new();
        for (transaction, transaction_output) in
            transactions.into_iter().zip(transaction_outputs.into_iter())
        {
            result.push((transaction, transaction_output));
        }
        Ok(result)
    }
}

impl FetchBlockDataFromDb for ApiContractClass {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, P2PSyncServerError> {
        let thin_state_diff =
            txn.get_state_diff(block_number)?.ok_or(P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        let declared_classes = thin_state_diff.declared_classes;
        let deprecated_declared_classes = thin_state_diff.deprecated_declared_classes;
        let mut result = Vec::new();
        for class_hash in &deprecated_declared_classes {
            result.push(ApiContractClass::DeprecatedContractClass(
                txn.get_deprecated_class(class_hash)?
                    .ok_or(P2PSyncServerError::ClassNotFound { class_hash: *class_hash })?,
            ));
        }
        for (class_hash, _) in &declared_classes {
            result.push(ApiContractClass::ContractClass(
                txn.get_class(class_hash)?
                    .ok_or(P2PSyncServerError::ClassNotFound { class_hash: *class_hash })?,
            ));
        }
        Ok(result)
    }
}

impl FetchBlockDataFromDb for (Event, TransactionHash) {
    fn fetch_block_data_from_db(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Vec<Self>, P2PSyncServerError> {
        let transaction_outputs = txn.get_block_transaction_outputs(block_number)?.ok_or(
            P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;
        let transaction_hashes = txn.get_block_transaction_hashes(block_number)?.ok_or(
            P2PSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;

        let mut result = Vec::new();
        for (transaction_output, transaction_hash) in
            transaction_outputs.into_iter().zip(transaction_hashes)
        {
            for event in transaction_output.events() {
                result.push((event.clone(), transaction_hash));
            }
        }
        Ok(result)
    }
}

pub fn split_thin_state_diff(thin_state_diff: ThinStateDiff) -> Vec<StateDiffChunk> {
    let mut state_diff_chunks = Vec::new();
    #[cfg(not(test))]
    let mut contract_addresses = std::collections::HashSet::new();
    #[cfg(test)]
    let mut contract_addresses = std::collections::BTreeSet::new();

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
) -> Result<(), P2PSyncServerError>
where
    Data: FetchBlockDataFromDb + Send + 'static,
    Sender: Sink<DataOrFin<Data>> + Unpin + Send + 'static,
    P2PSyncServerError: From<<Sender as Sink<DataOrFin<Data>>>::Error>,
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
) -> Result<(), P2PSyncServerError>
where
    Data: FetchBlockDataFromDb + Send + 'static,
    Sender: Sink<DataOrFin<Data>> + Unpin + Send + 'static,
    P2PSyncServerError: From<<Sender as Sink<DataOrFin<Data>>>::Error>,
{
    let txn = storage_reader.begin_ro_txn()?;
    let start_block_number = match query.start_block {
        BlockHashOrNumber::Number(BlockNumber(num)) => num,
        BlockHashOrNumber::Hash(block_hash) => {
            txn.get_block_number_by_hash(&block_hash)?
                .ok_or(P2PSyncServerError::BlockNotFound {
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
