mod api;
mod block;
mod deprecated_contract_class;
#[cfg(test)]
mod gateway_test;
mod state;
#[cfg(test)]
mod test_utils;
mod transaction;

use std::collections::HashMap;
use std::fmt::Display;
use std::net::SocketAddr;
use std::sync::Arc;

use api::GatewayContractClass;
use blockifier::execution::entry_point::{CallEntryPoint, ExecutionContext, ExecutionResources};
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::DictStateReader;
use blockifier::transaction::objects::AccountTransactionContext;
use jsonrpsee::core::{async_trait, Error};
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::http_server::{HttpServerBuilder, HttpServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::{ErrorObject, INTERNAL_ERROR_MSG};
use papyrus_storage::body::events::EventsReader;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{EventIndex, StorageReader, StorageTxn, TransactionIndex};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockStatus};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, GlobalRoot, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash, GENESIS_HASH};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_api::transaction::{
    Calldata, EventIndexInTransactionOutput, Fee, TransactionHash, TransactionOffsetInBlock,
};
use state::{FunctionCall, FunctionCallResult};
use tracing::{debug, error, info, instrument};

use crate::api::{
    BlockHashAndNumber, BlockHashOrNumber, BlockId, ContinuationToken, EventFilter, EventsChunk,
    JsonRpcError, JsonRpcServer, Tag,
};
use crate::block::{Block, BlockHeader};
use crate::state::StateUpdate;
use crate::transaction::{
    Event, Transaction, TransactionOutput, TransactionReceipt, TransactionReceiptWithStatus,
    TransactionStatus, TransactionWithType, Transactions,
};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GatewayConfig {
    pub chain_id: ChainId,
    pub server_address: String,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    pub fee_address: String,
}

/// Rpc server.
struct JsonRpcServerImpl {
    chain_id: ChainId,
    storage_reader: StorageReader,
    max_events_chunk_size: usize,
    max_events_keys: usize,
    fee_token_address: ContractAddress,
}

impl From<JsonRpcError> for Error {
    fn from(err: JsonRpcError) -> Self {
        Error::Call(CallError::Custom(ErrorObject::owned(err as i32, err.to_string(), None::<()>)))
    }
}

fn internal_server_error(err: impl Display) -> Error {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    Error::Call(CallError::Custom(ErrorObject::owned(
        InternalError.code(),
        INTERNAL_ERROR_MSG,
        None::<()>,
    )))
}

fn get_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_id: BlockId,
) -> Result<BlockNumber, Error> {
    Ok(match block_id {
        BlockId::HashOrNumber(BlockHashOrNumber::Hash(block_hash)) => txn
            .get_block_number_by_hash(&block_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?,
        BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number)) => {
            // Check that the block exists.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;
            if block_number > last_block_number {
                return Err(Error::from(JsonRpcError::BlockNotFound));
            }
            block_number
        }
        BlockId::Tag(Tag::Latest) => {
            get_latest_block_number(txn)?.ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?
        }
        BlockId::Tag(Tag::Pending) => {
            todo!("Pending tag is not supported yet.")
        }
    })
}

fn get_latest_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, Error> {
    Ok(txn.get_header_marker().map_err(internal_server_error)?.prev())
}

fn get_block_header_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockHeader, Error> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;

    Ok(BlockHeader::from(header))
}

fn get_block_txs_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<Transaction>, Error> {
    let transactions = txn
        .get_block_transactions(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;

    Ok(transactions.into_iter().map(Transaction::from).collect())
}

fn get_state<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<blockifier::state::cached_state::CachedState<DictStateReader>, Error> {
    let state_reader = txn.get_state_reader().unwrap();

    let class_hash_classes = state_reader.get_class_hashes_classes(block_number).unwrap();

    let res = class_hash_classes.into_iter().fold(
        HashMap::new(),
        |mut acc: HashMap<ClassHash, blockifier::execution::contract_class::ContractClass>,
         (key, v)| {
            if let Ok(contract_class) =
                blockifier::execution::contract_class::ContractClass::try_from(v)
            {
                acc.insert(key, contract_class);
            }

            acc
        },
    );

    let state = blockifier::state::cached_state::CachedState::new(DictStateReader {
        storage_view: state_reader.get_storage_view(block_number).unwrap(),
        address_to_nonce: state_reader.get_addresses_nonces().unwrap(),
        address_to_class_hash: state_reader.get_adresses_class_hashes(block_number).unwrap(),
        class_hash_to_class: res,
    });

    return Ok(state);
}

struct ContinuationTokenAsStruct(EventIndex);

impl ContinuationToken {
    fn parse(&self) -> Result<ContinuationTokenAsStruct, Error> {
        let ct = serde_json::from_str(&self.0)
            .map_err(|_| Error::from(JsonRpcError::InvalidContinuationToken))?;

        Ok(ContinuationTokenAsStruct(ct))
    }

    fn new(ct: ContinuationTokenAsStruct) -> Result<Self, Error> {
        Ok(Self(serde_json::to_string(&ct.0).map_err(internal_server_error)?))
    }
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    #[instrument(skip(self), level = "debug", err, ret)]
    fn block_number(&self) -> Result<BlockNumber, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        get_latest_block_number(&txn)?.ok_or_else(|| Error::from(JsonRpcError::NoBlocks))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn block_hash_and_number(&self) -> Result<BlockHashAndNumber, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number =
            get_latest_block_number(&txn)?.ok_or_else(|| Error::from(JsonRpcError::NoBlocks))?;
        let header = get_block_header_by_number(&txn, block_number)?;

        Ok(BlockHashAndNumber { block_hash: header.block_hash, block_number })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> Result<Block, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let header = get_block_header_by_number(&txn, block_number)?;
        let transactions = get_block_txs_by_number(&txn, block_number)?;
        let transaction_hashes: Vec<TransactionHash> =
            transactions.iter().map(|transaction| transaction.transaction_hash()).collect();

        Ok(Block {
            status: BlockStatus::AcceptedOnL2,
            header,
            transactions: Transactions::Hashes(transaction_hashes),
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_w_full_transactions(&self, block_id: BlockId) -> Result<Block, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let header = get_block_header_by_number(&txn, block_number)?;
        let transactions = get_block_txs_by_number(&txn, block_number)?;

        Ok(Block {
            status: BlockStatus::AcceptedOnL2,
            header,
            transactions: Transactions::Full(
                transactions.into_iter().map(TransactionWithType::from).collect(),
            ),
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> Result<StarkFelt, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Check that the block is valid and get the state number.
        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // Check that the contract exists.
        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        state_reader.get_storage_at(state, &contract_address, &key).map_err(internal_server_error)
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionWithType, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let transaction_index = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let transaction = txn
            .get_transaction(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        Ok(TransactionWithType::from(transaction))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> Result<TransactionWithType, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;

        let transaction = txn
            .get_transaction(TransactionIndex(block_number, index))
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionIndex))?;

        Ok(TransactionWithType::from(transaction))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_transaction_count(&self, block_id: BlockId) -> Result<usize, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let transactions = get_block_txs_by_number(&txn, block_number)?;

        Ok(transactions.len())
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_state_update(&self, block_id: BlockId) -> Result<StateUpdate, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Get the block header for the block hash and state root.
        let block_number = get_block_number(&txn, block_id)?;
        let header = get_block_header_by_number(&txn, block_number)?;

        // Get the old root.
        let parent_block_number = get_block_number(
            &txn,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.parent_hash)),
        );
        let mut old_root =
            GlobalRoot(StarkHash::try_from(GENESIS_HASH).map_err(internal_server_error)?);
        if parent_block_number.is_ok() {
            let parent_header = get_block_header_by_number(&txn, parent_block_number.unwrap())?;
            old_root = parent_header.new_root;
        }

        // Get the block state diff.
        let thin_state_diff = txn
            .get_state_diff(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;

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
    ) -> Result<TransactionReceiptWithStatus, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let transaction_index = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let block_number = transaction_index.0;
        let header =
            get_block_header_by_number(&txn, block_number).map_err(internal_server_error)?;

        let transaction = txn
            .get_transaction(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let thin_tx_output = txn
            .get_transaction_output(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let events = txn
            .get_transaction_events(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let output = TransactionOutput::from_thin_transaction_output(thin_tx_output, events);

        Ok(TransactionReceiptWithStatus {
            receipt: TransactionReceipt::from_transaction_output(
                output,
                &transaction,
                header.block_hash,
                block_number,
            ),
            status: TransactionStatus::default(),
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> Result<GatewayContractClass, Error> {
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
                .ok_or_else(|| Error::from(JsonRpcError::ClassHashNotFound))?;
            Ok(GatewayContractClass::Cairo0(class.try_into().map_err(internal_server_error)?))
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<GatewayContractClass, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        let class_hash = state_reader
            .get_class_hash_at(state_number, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        if let Some(class) = state_reader
            .get_class_definition_at(state_number, &class_hash)
            .map_err(internal_server_error)?
        {
            Ok(GatewayContractClass::Sierra(class.try_into().map_err(internal_server_error)?))
        } else {
            let class = state_reader
                .get_deprecated_class_definition_at(state_number, &class_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;
            Ok(GatewayContractClass::Cairo0(class.try_into().map_err(internal_server_error)?))
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_nonce(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<Nonce, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_nonce_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn chain_id(&self) -> Result<String, Error> {
        Ok(self.chain_id.as_hex())
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_events(&self, filter: EventFilter) -> Result<EventsChunk, Error> {
        // Check the chunk size.
        if filter.chunk_size > self.max_events_chunk_size {
            return Err(Error::from(JsonRpcError::PageSizeTooBig));
        }
        // Check the number of keys.
        if filter.keys.len() > self.max_events_keys {
            return Err(Error::from(JsonRpcError::TooManyKeysInFilter));
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
                let header = get_block_header_by_number(&txn, block_number)
                    .map_err(internal_server_error)?;
                let transaction = txn
                    .get_transaction(event_index.0)
                    .map_err(internal_server_error)?
                    .ok_or_else(|| internal_server_error("Unknown internal error."))?;
                let emitted_event = Event {
                    block_hash: header.block_hash,
                    block_number,
                    transaction_hash: transaction.transaction_hash(),
                    event: starknet_api::transaction::Event { from_address, content },
                };
                filtered_events.push(emitted_event);
            }
        }

        Ok(EventsChunk { events: filtered_events, continuation_token: None })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn call(&self, block_id: BlockId, request: FunctionCall) -> Result<FunctionCallResult, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let block_header = get_block_header_by_number(&txn, block_number)?;

        let mut state = get_state(&txn, block_number)?;

        let block_context = blockifier::block_context::BlockContext {
            chain_id: self.chain_id.clone(),
            block_number: block_header.block_number,
            block_timestamp: block_header.timestamp,
            cairo_resource_fee_weights: HashMap::default(),
            invoke_tx_max_n_steps: 1000000,
            validate_max_n_steps: 1000000,
            gas_price: 0,
            sequencer_address: block_header.sequencer_address,
            fee_token_address: self.fee_token_address.clone(),
        };

        let mut execution_resources = ExecutionResources::default();
        let mut execution_context = ExecutionContext::default();
        let account_context = AccountTransactionContext::default();

        let class_hash =
            state.get_class_hash_at(request.contract_address).map_err(internal_server_error)?;

        let call_entry_point = CallEntryPoint {
            class_hash: Option::Some(class_hash),
            entry_point_type: starknet_api::deprecated_contract_class::EntryPointType::External,
            entry_point_selector: request.entry_point_selector,
            calldata: request.calldata,
            call_type: blockifier::execution::entry_point::CallType::Call,
            storage_address: request.contract_address,
            caller_address: ContractAddress::default(),
        };

        let call_result = call_entry_point
            .execute(
                &mut state,
                &mut execution_resources,
                &mut execution_context,
                &block_context,
                &account_context,
            )
            .map_err(internal_server_error)?;

        Ok(FunctionCallResult(Arc::new(call_result.execution.retdata.0)))
    }
}

#[instrument(skip(storage_reader), level = "debug", err)]
pub async fn run_server(
    config: &GatewayConfig,
    storage_reader: StorageReader,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    debug!("Starting gateway.");
    let server = HttpServerBuilder::default().build(&config.server_address).await?;
    let addr = server.local_addr()?;
    let handle = server.start(
        JsonRpcServerImpl {
            chain_id: config.chain_id.clone(),
            storage_reader,
            max_events_chunk_size: config.max_events_chunk_size,
            max_events_keys: config.max_events_keys,
            fee_token_address: ContractAddress::try_from(StarkFelt::try_from(
                config.fee_address.as_str(),
            )?)?,
        }
        .into_rpc(),
    )?;
    info!(local_address = %addr, "Gateway is running.");
    Ok((addr, handle))
}
