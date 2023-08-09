use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use papyrus_execution::{execute_call, ExecutionError};
use papyrus_storage::body::events::{EventIndex, EventsReader};
use papyrus_storage::body::{BodyStorageReader, TransactionIndex};
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockNumber, BlockStatus};
use starknet_api::core::{
    ChainId,
    ClassHash,
    ContractAddress,
    EntryPointSelector,
    GlobalRoot,
    Nonce,
};
use starknet_api::hash::{StarkFelt, StarkHash, GENESIS_HASH};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_api::transaction::{
    Calldata,
    EventIndexInTransactionOutput,
    TransactionHash,
    TransactionOffsetInBlock,
};
use starknet_client::writer::{StarknetWriter, WriterClientError};
use starknet_client::ClientError;
use tokio::sync::RwLock;
use tracing::instrument;

use super::super::block::{Block, BlockHeader};
use super::super::state::StateUpdate;
use super::super::transaction::{
    Event,
    Transaction,
    TransactionOutput,
    TransactionReceipt,
    TransactionWithHash,
    Transactions,
};
use super::{
    BlockHashAndNumber,
    BlockId,
    ContinuationToken,
    EventFilter,
    EventsChunk,
    GatewayContractClass,
    JsonRpcV0_4Server,
};
use crate::api::{BlockHashOrNumber, JsonRpcServerImpl};
use crate::syncing_state::{get_last_synced_block, SyncStatus, SyncingState};
use crate::v0_4_0::block::{get_block_header_by_number, get_block_number};
use crate::v0_4_0::error::{
    JsonRpcError,
    BLOCK_NOT_FOUND,
    CLASS_HASH_NOT_FOUND,
    CONTRACT_ERROR,
    CONTRACT_NOT_FOUND,
    INVALID_TRANSACTION_INDEX,
    NO_BLOCKS,
    PAGE_SIZE_TOO_BIG,
    TOO_MANY_KEYS_IN_FILTER,
    TRANSACTION_HASH_NOT_FOUND,
};
use crate::v0_4_0::transaction::{
    get_block_tx_hashes_by_number, get_block_txs_by_number, InvokeTransactionV1,
};
use crate::v0_4_0::write_api_error::starknet_error_to_invoke_error;
use crate::v0_4_0::write_api_result::AddInvokeOkResult;
use crate::{
    get_block_status,
    get_latest_block_number,
    internal_server_error,
    ContinuationTokenAsStruct,
};

/// Rpc server.
pub struct JsonRpcServerV0_4Impl {
    pub chain_id: ChainId,
    pub storage_reader: StorageReader,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    pub starting_block: BlockHashAndNumber,
    pub shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pub writer_client: Arc<dyn StarknetWriter>,
}

#[async_trait]
impl JsonRpcV0_4Server for JsonRpcServerV0_4Impl {
    #[instrument(skip(self), level = "debug", err, ret)]
    fn block_number(&self) -> RpcResult<BlockNumber> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        get_latest_block_number(&txn)?.ok_or_else(|| ErrorObjectOwned::from(NO_BLOCKS))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn block_hash_and_number(&self) -> RpcResult<BlockHashAndNumber> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number =
            get_latest_block_number(&txn)?.ok_or_else(|| ErrorObjectOwned::from(NO_BLOCKS))?;
        let header: BlockHeader = get_block_header_by_number(&txn, block_number)?;

        Ok(BlockHashAndNumber { block_hash: header.block_hash, block_number })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> RpcResult<Block> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let status = get_block_status(&txn, block_number)?;
        let header = get_block_header_by_number(&txn, block_number)?;
        let transaction_hashes = get_block_tx_hashes_by_number(&txn, block_number)?;

        Ok(Block { status, header, transactions: Transactions::Hashes(transaction_hashes) })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_w_full_transactions(&self, block_id: BlockId) -> RpcResult<Block> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let status = get_block_status(&txn, block_number)?;
        let header = get_block_header_by_number(&txn, block_number)?;
        // TODO(dvir): consider create a vector of (transaction, transaction_index) first and get
        // the transaction hashes by the index.
        let transactions = get_block_txs_by_number(&txn, block_number)?;
        let transaction_hashes = get_block_tx_hashes_by_number(&txn, block_number)?;
        let transactions_with_hash = transactions
            .into_iter()
            .zip(transaction_hashes)
            .map(|(transaction, transaction_hash)| TransactionWithHash {
                transaction,
                transaction_hash,
            })
            .collect();

        Ok(Block { status, header, transactions: Transactions::Full(transactions_with_hash) })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> RpcResult<StarkFelt> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Check that the block is valid and get the state number.
        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // Check that the contract exists.
        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))?;

        state_reader.get_storage_at(state, &contract_address, &key).map_err(internal_server_error)
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionWithHash> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let transaction_index = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;

        let transaction = txn
            .get_transaction(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;

        Ok(TransactionWithHash { transaction: transaction.into(), transaction_hash })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> RpcResult<TransactionWithHash> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;

        let tx_index = TransactionIndex(block_number, index);
        let transaction = txn
            .get_transaction(tx_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(INVALID_TRANSACTION_INDEX))?;
        let transaction_hash = txn
            .get_transaction_hash_by_idx(&tx_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(INVALID_TRANSACTION_INDEX))?;

        Ok(TransactionWithHash { transaction: transaction.into(), transaction_hash })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_transaction_count(&self, block_id: BlockId) -> RpcResult<usize> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let transactions: Vec<Transaction> = get_block_txs_by_number(&txn, block_number)?;

        Ok(transactions.len())
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_state_update(&self, block_id: BlockId) -> RpcResult<StateUpdate> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Get the block header for the block hash and state root.
        let block_number = get_block_number(&txn, block_id)?;
        let header: BlockHeader = get_block_header_by_number(&txn, block_number)?;

        // Get the old root.
        let parent_block_number = get_block_number(
            &txn,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.parent_hash)),
        );
        let mut old_root =
            GlobalRoot(StarkHash::try_from(GENESIS_HASH).map_err(internal_server_error)?);
        if parent_block_number.is_ok() {
            let parent_header: BlockHeader =
                get_block_header_by_number(&txn, parent_block_number.unwrap())?;
            old_root = parent_header.new_root;
        }

        // Get the block state diff.
        let thin_state_diff = txn
            .get_state_diff(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;

        Ok(StateUpdate {
            block_hash: header.block_hash,
            new_root: header.new_root,
            old_root,
            state_diff: thin_state_diff.into(),
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionReceipt> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let transaction_index = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;

        let block_number = transaction_index.0;
        let status = get_block_status(&txn, block_number)?;

        // rejected blocks should not be a part of the API so we early return here.
        // this assumption also holds for the conversion from block status to transaction finality
        // status where we set rejected blocks to unreachable.
        if status == BlockStatus::Rejected {
            return Err(ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;
        }

        let block_hash = get_block_header_by_number::<_, BlockHeader>(&txn, block_number)
            .map_err(internal_server_error)?
            .block_hash;

        let thin_tx_output = txn
            .get_transaction_output(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;

        let events = txn
            .get_transaction_events(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;

        let output = TransactionOutput::from_thin_transaction_output(thin_tx_output, events);

        Ok(TransactionReceipt {
            finality_status: status.into(),
            transaction_hash,
            block_hash,
            block_number,
            output,
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> RpcResult<GatewayContractClass> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // The class might be a deprecated class. Search it first in the declared classes and if not
        // found, search in the deprecated classes.
        if let Some(class) = state_reader
            .get_class_definition_at(state_number, &class_hash)
            .map_err(internal_server_error)?
        {
            Ok(GatewayContractClass::Sierra(class.try_into().map_err(internal_server_error)?))
        } else {
            let class = state_reader
                .get_deprecated_class_definition_at(state_number, &class_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(CLASS_HASH_NOT_FOUND))?;
            Ok(GatewayContractClass::Cairo0(class.try_into().map_err(internal_server_error)?))
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<GatewayContractClass> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        let class_hash = state_reader
            .get_class_hash_at(state_number, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))?;

        if let Some(class) = state_reader
            .get_class_definition_at(state_number, &class_hash)
            .map_err(internal_server_error)?
        {
            Ok(GatewayContractClass::Sierra(class.try_into().map_err(internal_server_error)?))
        } else {
            let class = state_reader
                .get_deprecated_class_definition_at(state_number, &class_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))?;
            Ok(GatewayContractClass::Cairo0(class.try_into().map_err(internal_server_error)?))
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<ClassHash> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_nonce(&self, block_id: BlockId, contract_address: ContractAddress) -> RpcResult<Nonce> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_nonce_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn chain_id(&self) -> RpcResult<String> {
        Ok(self.chain_id.as_hex())
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_events(&self, filter: EventFilter) -> RpcResult<EventsChunk> {
        // Check the chunk size.
        if filter.chunk_size > self.max_events_chunk_size {
            return Err(ErrorObjectOwned::from(PAGE_SIZE_TOO_BIG));
        }
        // Check the number of keys.
        if filter.keys.len() > self.max_events_keys {
            return Err(ErrorObjectOwned::from(TOO_MANY_KEYS_IN_FILTER));
        }

        // Get the requested block numbers.
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let from_block_number = filter
            .from_block
            .map_or(Ok(BlockNumber(0)), |block_id| get_block_number(&txn, block_id))?;
        let maybe_to_block_number =
            filter.to_block.map_or(get_latest_block_number(&txn), |block_id| {
                get_block_number(&txn, block_id).map(Some)
            })?;
        if maybe_to_block_number.is_none() {
            // There are no blocks.
            return Ok(EventsChunk { events: vec![], continuation_token: None });
        }
        let to_block_number = maybe_to_block_number.unwrap();
        if from_block_number > to_block_number {
            return Ok(EventsChunk { events: vec![], continuation_token: None });
        }

        // Get the event index. If there's a continuation token we take the event index from there.
        // Otherwise, we take the first index in the from_block_number.
        let event_index = match filter.continuation_token {
            Some(token) => token.parse()?.0,
            None => EventIndex(
                TransactionIndex(from_block_number, TransactionOffsetInBlock(0)),
                EventIndexInTransactionOutput(0),
            ),
        };

        // Collect the requested events.
        // Once we collected enough events, we continue to check if there are any more events
        // corresponding to the requested filter. If there are, we return a continuation token
        // pointing to the next relevant event. Otherwise, we return a continuation token None.
        let mut filtered_events = vec![];
        for ((from_address, event_index), content) in txn
            .iter_events(filter.address, event_index, to_block_number)
            .map_err(internal_server_error)?
        {
            let block_number = (event_index.0).0;
            if block_number > to_block_number {
                break;
            }
            if filter.address.is_some() && from_address != filter.address.unwrap() {
                break;
            }
            // TODO: Consider changing empty sets in the filer keys to None.
            if filter.keys.iter().enumerate().all(|(i, keys)| {
                content.keys.len() > i && (keys.is_empty() || keys.contains(&content.keys[i]))
            }) {
                if filtered_events.len() == filter.chunk_size {
                    return Ok(EventsChunk {
                        events: filtered_events,
                        continuation_token: Some(ContinuationToken::new(
                            ContinuationTokenAsStruct(event_index),
                        )?),
                    });
                }
                let header: BlockHeader = get_block_header_by_number(&txn, block_number)
                    .map_err(internal_server_error)?;
                let transaction_hash = txn
                    .get_transaction_hash_by_idx(&event_index.0)
                    .map_err(internal_server_error)?
                    .ok_or_else(|| internal_server_error("Unknown internal error."))?;
                let emitted_event = Event {
                    block_hash: header.block_hash,
                    block_number,
                    transaction_hash,
                    event: starknet_api::transaction::Event { from_address, content },
                };
                filtered_events.push(emitted_event);
            }
        }

        Ok(EventsChunk { events: filtered_events, continuation_token: None })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn syncing(&self) -> RpcResult<SyncingState> {
        let Some(highest_block) = *self.shared_highest_block.read().await else {
            return Ok(SyncingState::Synced);
        };
        let current_block =
            get_last_synced_block(self.storage_reader.clone()).map_err(internal_server_error)?;
        if highest_block.block_number <= current_block.block_number {
            return Ok(SyncingState::Synced);
        }
        Ok(SyncingState::SyncStatus(SyncStatus {
            starting_block_hash: self.starting_block.block_hash,
            starting_block_num: self.starting_block.block_number,
            current_block_hash: current_block.block_hash,
            current_block_num: current_block.block_number,
            highest_block_hash: highest_block.block_hash,
            highest_block_num: highest_block.block_number,
        }))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn call(
        &self,
        contract_address: ContractAddress,
        entry_point_selector: EntryPointSelector,
        calldata: Calldata,
        block_id: BlockId,
    ) -> RpcResult<Vec<StarkFelt>> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        match execute_call(
            &txn,
            &self.chain_id,
            state_number,
            &contract_address,
            entry_point_selector,
            calldata,
        ) {
            Ok(res) => Ok(res.retdata.0),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn add_invoke_transaction(
        &self,
        invoke_transaction: InvokeTransactionV1,
    ) -> RpcResult<AddInvokeOkResult> {
        let result = self.writer_client.add_invoke_transaction(&invoke_transaction.into()).await;
        match result {
            Ok(res) => Ok(res.into()),
            Err(WriterClientError::ClientError(ClientError::StarknetError(starknet_error))) => {
                Err(ErrorObjectOwned::from(starknet_error_to_invoke_error(starknet_error)))
            }
            Err(err) => Err(internal_server_error(err)),
        }
    }
}

impl JsonRpcServerImpl for JsonRpcServerV0_4Impl {
    fn new(
        chain_id: ChainId,
        storage_reader: StorageReader,
        max_events_chunk_size: usize,
        max_events_keys: usize,
        starting_block: BlockHashAndNumber,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        writer_client: Arc<dyn StarknetWriter>,
    ) -> Self {
        Self {
            chain_id,
            storage_reader,
            max_events_chunk_size,
            max_events_keys,
            starting_block,
            shared_highest_block,
            writer_client,
        }
    }

    fn into_rpc_module(self) -> RpcModule<Self> {
        self.into_rpc()
    }
}

impl TryFrom<ExecutionError> for JsonRpcError {
    type Error = ErrorObjectOwned;
    fn try_from(value: ExecutionError) -> Result<Self, Self::Error> {
        match value {
            ExecutionError::NotSynced { .. } => Ok(BLOCK_NOT_FOUND),
            ExecutionError::ContractNotFound { .. } => Ok(CONTRACT_NOT_FOUND),
            // All other execution errors are considered contract errors.
            _ => Ok(CONTRACT_ERROR),
        }
    }
}
