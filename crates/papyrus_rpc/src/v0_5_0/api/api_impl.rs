use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use lazy_static::lazy_static;
use papyrus_execution::objects::TransactionTrace;
use papyrus_execution::{
    estimate_fee as exec_estimate_fee,
    execute_call,
    simulate_transactions as exec_simulate_transactions,
    ExecutionConfigByBlock,
    ExecutionError,
};
use papyrus_storage::body::events::{EventIndex, EventsReader, ExecutionResources};
use papyrus_storage::body::{BodyStorageReader, TransactionIndex};
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
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
use tracing::{debug, instrument, trace};

use super::super::block::{Block, BlockHeader};
use super::super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedTransaction,
};
use super::super::state::{GatewayStateUpdate, StateUpdate};
use super::super::transaction::{
    Event,
    Transaction,
    TransactionOutput,
    TransactionReceipt,
    TransactionWithHash,
    Transactions,
};
use super::{
    stored_txn_to_executable_txn,
    BlockHashAndNumber,
    BlockId,
    ContinuationToken,
    EventFilter,
    EventsChunk,
    FeeEstimate,
    GatewayContractClass,
    JsonRpcV0_5Server,
    SimulatedTransaction,
    SimulationFlag,
    TransactionTraceWithHash,
};
use crate::api::{BlockHashOrNumber, JsonRpcServerImpl};
use crate::syncing_state::{get_last_synced_block, SyncStatus, SyncingState};
use crate::v0_5_0::block::{get_block_header_by_number, get_block_number};
use crate::v0_5_0::error::{
    JsonRpcError,
    BLOCK_NOT_FOUND,
    CLASS_HASH_NOT_FOUND,
    CONTRACT_NOT_FOUND,
    INVALID_BLOCK_HASH,
    INVALID_TRANSACTION_HASH,
    INVALID_TRANSACTION_INDEX,
    NO_BLOCKS,
    PAGE_SIZE_TOO_BIG,
    TOO_MANY_KEYS_IN_FILTER,
    TRANSACTION_HASH_NOT_FOUND,
};
use crate::v0_5_0::transaction::{
    get_block_tx_hashes_by_number,
    get_block_txs_by_number,
    DeployAccountTransaction,
    InvokeTransactionV1,
};
use crate::v0_5_0::write_api_error::{
    starknet_error_to_declare_error,
    starknet_error_to_deploy_account_error,
    starknet_error_to_invoke_error,
};
use crate::v0_5_0::write_api_result::{
    AddDeclareOkResult,
    AddDeployAccountOkResult,
    AddInvokeOkResult,
};
use crate::{
    get_block_status,
    get_latest_block_number,
    internal_server_error,
    ContinuationTokenAsStruct,
};

// TODO(yael): implement address 0x1 as a const function in starknet_api.
lazy_static! {
    pub static ref BLOCK_HASH_TABLE_ADDRESS: ContractAddress = ContractAddress::from(1_u8);
}

/// Rpc server.
pub struct JsonRpcServerV0_5Impl {
    pub chain_id: ChainId,
    pub execution_config: ExecutionConfigByBlock,
    pub storage_reader: StorageReader,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    pub starting_block: BlockHashAndNumber,
    pub shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pub writer_client: Arc<dyn StarknetWriter>,
}

#[async_trait]
impl JsonRpcV0_5Server for JsonRpcServerV0_5Impl {
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

        let res = state_reader
            .get_storage_at(state, &contract_address, &key)
            .map_err(internal_server_error)?;
        // Contract address 0x1 is a special address, it stores the block
        // hashes. Contracts are not deployed to this address.
        if res == StarkFelt::default() && contract_address != *BLOCK_HASH_TABLE_ADDRESS {
            // check if the contract exists
            state_reader
                .get_class_hash_at(state, &contract_address)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))?;
        }
        Ok(res)
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

        Ok(TransactionWithHash { transaction: transaction.try_into()?, transaction_hash })
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

        Ok(TransactionWithHash { transaction: transaction.try_into()?, transaction_hash })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_block_transaction_count(&self, block_id: BlockId) -> RpcResult<usize> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let transactions: Vec<Transaction> = get_block_txs_by_number(&txn, block_number)?;

        Ok(transactions.len())
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn get_state_update(&self, block_id: BlockId) -> RpcResult<GatewayStateUpdate> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Get the block header for the block hash and state root.
        let block_number = get_block_number(&txn, block_id)?;
        let header: BlockHeader = get_block_header_by_number(&txn, block_number)?;

        // Get the old root.
        let old_root = match get_block_number(
            &txn,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.parent_hash)),
        ) {
            Ok(parent_block_number) => {
                get_block_header_by_number::<_, BlockHeader>(&txn, parent_block_number)?.new_root
            }
            Err(_) => GlobalRoot(StarkHash::try_from(GENESIS_HASH).map_err(internal_server_error)?),
        };

        // Get the block state diff.
        let thin_state_diff = txn
            .get_state_diff(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;

        let state_update = StateUpdate {
            block_hash: header.block_hash,
            new_root: header.new_root,
            old_root,
            state_diff: thin_state_diff.into(),
        };

        Ok(GatewayStateUpdate::StateUpdate(state_update))
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

        let execution_resources = ExecutionResources::default();

        Ok(TransactionReceipt {
            finality_status: status.into(),
            transaction_hash,
            block_hash,
            block_number,
            execution_resources,
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
        let Some(to_block_number) = maybe_to_block_number else {
            // There are no blocks.
            return Ok(EventsChunk { events: vec![], continuation_token: None });
        };
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
            if let Some(filter_address) = filter.address {
                if from_address != filter_address {
                    break;
                }
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
        let state_number = StateNumber::right_before_block(block_number);
        let block_execution_config =
            self.execution_config.get_execution_config_for_block(block_number).map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?;

        match execute_call(
            &txn,
            &self.chain_id,
            state_number,
            &contract_address,
            entry_point_selector,
            calldata,
            block_execution_config,
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

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn add_deploy_account_transaction(
        &self,
        deploy_account_transaction: DeployAccountTransaction,
    ) -> RpcResult<AddDeployAccountOkResult> {
        let result = self
            .writer_client
            .add_deploy_account_transaction(&deploy_account_transaction.into())
            .await;
        match result {
            Ok(res) => Ok(res.into()),
            Err(WriterClientError::ClientError(ClientError::StarknetError(starknet_error))) => {
                Err(ErrorObjectOwned::from(starknet_error_to_deploy_account_error(starknet_error)))
            }
            Err(err) => Err(internal_server_error(err)),
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn add_declare_transaction(
        &self,
        declare_transaction: BroadcastedDeclareTransaction,
    ) -> RpcResult<AddDeclareOkResult> {
        let result = self
            .writer_client
            .add_declare_transaction(
                &declare_transaction.try_into().map_err(internal_server_error)?,
            )
            .await;
        match result {
            Ok(res) => Ok(res.into()),
            Err(WriterClientError::ClientError(ClientError::StarknetError(starknet_error))) => {
                Err(ErrorObjectOwned::from(starknet_error_to_declare_error(starknet_error)))
            }
            Err(err) => Err(internal_server_error(err)),
        }
    }

    #[instrument(skip(self, transactions), level = "debug", err, ret)]
    fn estimate_fee(
        &self,
        transactions: Vec<BroadcastedTransaction>,
        block_id: BlockId,
    ) -> RpcResult<Vec<FeeEstimate>> {
        trace!("Estimating fee of transactions: {:#?}", transactions);
        let executable_txns =
            transactions.into_iter().map(|tx| tx.try_into()).collect::<Result<_, _>>()?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_before_block(block_number);
        let block_execution_config =
            self.execution_config.get_execution_config_for_block(block_number).map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?;

        match exec_estimate_fee(
            executable_txns,
            &self.chain_id,
            &txn,
            state_number,
            block_execution_config,
        ) {
            Ok(fees) => Ok(fees
                .into_iter()
                .map(|(gas_price, fee)| FeeEstimate::from(gas_price, fee))
                .collect()),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self, transactions), level = "debug", err, ret)]
    fn simulate_transactions(
        &self,
        block_id: BlockId,
        transactions: Vec<BroadcastedTransaction>,
        simulation_flags: Vec<SimulationFlag>,
    ) -> RpcResult<Vec<SimulatedTransaction>> {
        trace!("Simulating transactions: {:#?}", transactions);
        let executable_txns =
            transactions.into_iter().map(|tx| tx.try_into()).collect::<Result<_, _>>()?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_before_block(block_number);
        let block_execution_config =
            self.execution_config.get_execution_config_for_block(block_number).map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?;

        let charge_fee = !simulation_flags.contains(&SimulationFlag::SkipFeeCharge);
        let validate = !simulation_flags.contains(&SimulationFlag::SkipValidate);

        let res = exec_simulate_transactions(
            executable_txns,
            None,
            &self.chain_id,
            &txn,
            state_number,
            block_execution_config,
            charge_fee,
            validate,
        );

        match res {
            Ok(simulation_results) => Ok(simulation_results
                .into_iter()
                .map(|(transaction_trace, gas_price, fee)| SimulatedTransaction {
                    transaction_trace,
                    fee_estimation: FeeEstimate::from(gas_price, fee),
                })
                .collect()),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    fn trace_transaction(&self, transaction_hash: TransactionHash) -> RpcResult<TransactionTrace> {
        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let TransactionIndex(block_number, tx_offset) = storage_txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or(INVALID_TRANSACTION_HASH)?;
        let block_execution_config =
            self.execution_config.get_execution_config_for_block(block_number).map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?;

        let casm_marker = storage_txn.get_compiled_class_marker().map_err(internal_server_error)?;
        if casm_marker <= block_number {
            debug!(
                ?transaction_hash,
                ?block_number,
                ?casm_marker,
                "Transaction is in the storage, but the compiled classes are not fully synced up \
                 to its block.",
            );
            return Err(INVALID_TRANSACTION_HASH.into());
        }

        let block_transactions = storage_txn
            .get_block_transactions(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| {
                internal_server_error(StorageError::DBInconsistency {
                    msg: format!("Missing block {block_number} transactions"),
                })
            })?;

        let tx_hashes = storage_txn
            .get_block_transaction_hashes(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| {
                internal_server_error(StorageError::DBInconsistency {
                    msg: format!("Missing block {block_number} transactions"),
                })
            })?;

        let state_number = StateNumber::right_before_block(block_number);
        let executable_txns = block_transactions
            .into_iter()
            .take(tx_offset.0 + 1)
            .map(|tx| stored_txn_to_executable_txn(tx, &storage_txn, state_number))
            .collect::<Result<_, _>>()?;

        let res = exec_simulate_transactions(
            executable_txns,
            Some(tx_hashes),
            &self.chain_id,
            &storage_txn,
            state_number,
            block_execution_config,
            true,
            true,
        );

        match res {
            Ok(mut simulation_results) => {
                Ok(simulation_results.pop().expect("Should have transaction exeuction result").0)
            }
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    fn trace_block_transactions(
        &self,
        block_id: BlockId,
    ) -> RpcResult<Vec<TransactionTraceWithHash>> {
        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&storage_txn, block_id)?;
        let block_execution_config =
            self.execution_config.get_execution_config_for_block(block_number).map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?;

        let casm_marker = storage_txn.get_compiled_class_marker().map_err(internal_server_error)?;
        if casm_marker <= block_number {
            debug!(
                ?block_id,
                ?casm_marker,
                "Block is in the storage, but the compiled classes are not fully synced.",
            );
            return Err(INVALID_BLOCK_HASH.into());
        }

        let block_transactions = storage_txn
            .get_block_transactions(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| {
                internal_server_error(StorageError::DBInconsistency {
                    msg: format!("Missing block {block_number} transactions"),
                })
            })?;

        let tx_hashes = storage_txn
            .get_block_transaction_hashes(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| {
                internal_server_error(StorageError::DBInconsistency {
                    msg: format!("Missing block {block_number} transactions"),
                })
            })?;

        let state_number = StateNumber::right_before_block(block_number);
        let executable_txns = block_transactions
            .into_iter()
            .map(|tx| stored_txn_to_executable_txn(tx, &storage_txn, state_number))
            .collect::<Result<_, _>>()?;

        let res = exec_simulate_transactions(
            executable_txns,
            Some(tx_hashes.clone()),
            &self.chain_id,
            &storage_txn,
            state_number,
            block_execution_config,
            true,
            true,
        );

        match res {
            Ok(simulation_results) => Ok(simulation_results
                .into_iter()
                .zip(tx_hashes)
                .map(|((trace_root, _, _), transaction_hash)| TransactionTraceWithHash {
                    transaction_hash,
                    trace_root,
                })
                .collect()),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    fn get_transaction_status(&self, transaction_hash: TransactionHash) -> RpcResult<String> {
        Err(ErrorObjectOwned::from(CONTRACT_NOT_FOUND))
    }

    #[instrument(skip(self), level = "debug", err)]
    fn spec_version(&self) -> RpcResult<String> {
        Ok("V0_5_0".to_string())
    }
}

impl JsonRpcServerImpl for JsonRpcServerV0_5Impl {
    fn new(
        chain_id: ChainId,
        execution_config: ExecutionConfigByBlock,
        storage_reader: StorageReader,
        max_events_chunk_size: usize,
        max_events_keys: usize,
        starting_block: BlockHashAndNumber,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        writer_client: Arc<dyn StarknetWriter>,
    ) -> Self {
        Self {
            chain_id,
            execution_config,
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
