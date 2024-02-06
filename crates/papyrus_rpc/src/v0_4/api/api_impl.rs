use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use lazy_static::lazy_static;
use papyrus_common::pending_classes::{PendingClasses, PendingClassesTrait};
use papyrus_execution::objects::{
    PendingData as ExecutionPendingData,
    TransactionSimulationOutput,
};
use papyrus_execution::{
    estimate_fee as exec_estimate_fee,
    execute_call,
    execution_utils,
    simulate_transactions as exec_simulate_transactions,
    ExecutableTransactionInput,
    ExecutionConfigByBlock,
    ExecutionError,
};
use papyrus_storage::body::events::{EventIndex, EventsReader};
use papyrus_storage::body::{BodyStorageReader, TransactionIndex};
use papyrus_storage::db::TransactionKind;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageTxn};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::hash::GENESIS_HASH;
use starknet_api::state::{StateNumber, StorageKey};
use starknet_api::transaction::{
    EventContent,
    EventIndexInTransactionOutput,
    Fee,
    Transaction as StarknetApiTransaction,
    TransactionHash,
    TransactionOffsetInBlock,
};
use starknet_client::reader::objects::pending_data::{
    PendingBlock,
    PendingStateUpdate as ClientPendingStateUpdate,
};
use starknet_client::reader::PendingData;
use starknet_client::writer::{StarknetWriter, WriterClientError};
use starknet_client::ClientError;
use starknet_types_core::felt::Felt;
use tokio::sync::RwLock;
use tracing::{instrument, trace, warn};

use super::super::block::{
    get_accepted_block_number,
    get_block_header_by_number,
    Block,
    BlockHeader,
    BlockNotRevertedValidator,
    GeneralBlockHeader,
    PendingBlockHeader,
};
use super::super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedTransaction,
};
use super::super::error::{
    JsonRpcError,
    BLOCK_NOT_FOUND,
    CLASS_HASH_NOT_FOUND,
    CONTRACT_ERROR,
    CONTRACT_NOT_FOUND,
    INVALID_TRANSACTION_HASH,
    INVALID_TRANSACTION_INDEX,
    NO_BLOCKS,
    PAGE_SIZE_TOO_BIG,
    TOO_MANY_KEYS_IN_FILTER,
    TRANSACTION_HASH_NOT_FOUND,
};
use super::super::execution::TransactionTrace;
use super::super::state::{AcceptedStateUpdate, PendingStateUpdate, StateUpdate};
use super::super::transaction::{
    get_block_tx_hashes_by_number,
    get_block_txs_by_number,
    Event,
    GeneralTransactionReceipt,
    MessageFromL1,
    PendingTransactionFinalityStatus,
    PendingTransactionOutput,
    PendingTransactionReceipt,
    TransactionOutput,
    TransactionReceipt,
    TransactionWithHash,
    Transactions,
    TypedDeployAccountTransaction,
    TypedInvokeTransactionV1,
};
use super::super::write_api_error::{
    starknet_error_to_declare_error,
    starknet_error_to_deploy_account_error,
    starknet_error_to_invoke_error,
};
use super::super::write_api_result::{
    AddDeclareOkResult,
    AddDeployAccountOkResult,
    AddInvokeOkResult,
};
use super::{
    stored_txn_to_executable_txn,
    BlockHashAndNumber,
    BlockId,
    CallRequest,
    ContinuationToken,
    EventFilter,
    EventsChunk,
    FeeEstimate,
    GatewayContractClass,
    JsonRpcV0_4Server,
    SimulatedTransaction,
    SimulationFlag,
    TransactionTraceWithHash,
};
use crate::api::{BlockHashOrNumber, JsonRpcServerImpl, Tag};
use crate::pending::client_pending_data_to_execution_pending_data;
use crate::syncing_state::{get_last_synced_block, SyncStatus, SyncingState};
use crate::{
    get_block_status,
    get_latest_block_number,
    internal_server_error,
    verify_storage_scope,
    ContinuationTokenAsStruct,
};

// TODO(yael): implement address 0x1 as a const function in starknet_api.
lazy_static! {
    pub static ref BLOCK_HASH_TABLE_ADDRESS: ContractAddress = ContractAddress::from(1_u8);
}

/// Rpc server.
pub struct JsonRpcServerV0_4Impl {
    pub chain_id: ChainId,
    pub execution_config: ExecutionConfigByBlock,
    pub storage_reader: StorageReader,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    pub starting_block: BlockHashAndNumber,
    pub shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pub pending_data: Arc<RwLock<PendingData>>,
    pub pending_classes: Arc<RwLock<PendingClasses>>,
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
    async fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> RpcResult<Block> {
        verify_storage_scope(&self.storage_reader)?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        if let BlockId::Tag(Tag::Pending) = block_id {
            let block = read_pending_data(&self.pending_data, &txn).await?.block;
            let pending_block_header = PendingBlockHeader {
                parent_hash: block.parent_block_hash,
                sequencer_address: block.sequencer_address,
                timestamp: block.timestamp,
            };
            let header = GeneralBlockHeader::PendingBlockHeader(pending_block_header);
            let client_transactions = block.transactions;
            let transaction_hashes = client_transactions
                .iter()
                .map(|transaction| transaction.transaction_hash())
                .collect();
            return Ok(Block {
                status: None,
                header,
                transactions: Transactions::Hashes(transaction_hashes),
            });
        }

        let block_number = get_accepted_block_number(&txn, block_id)?;
        let status = get_block_status(&txn, block_number)?;
        let header =
            GeneralBlockHeader::BlockHeader(get_block_header_by_number(&txn, block_number)?);
        let transaction_hashes = get_block_tx_hashes_by_number(&txn, block_number)?;

        Ok(Block {
            status: Some(status),
            header,
            transactions: Transactions::Hashes(transaction_hashes),
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_block_w_full_transactions(&self, block_id: BlockId) -> RpcResult<Block> {
        verify_storage_scope(&self.storage_reader)?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        if let BlockId::Tag(Tag::Pending) = block_id {
            let block = read_pending_data(&self.pending_data, &txn).await?.block;
            let pending_block_header = PendingBlockHeader {
                parent_hash: block.parent_block_hash,
                sequencer_address: block.sequencer_address,
                timestamp: block.timestamp,
            };
            let header = GeneralBlockHeader::PendingBlockHeader(pending_block_header);
            let client_transactions = block.transactions;
            let transactions = client_transactions
                .iter()
                .map(|client_transaction| {
                    let starknet_api_transaction: StarknetApiTransaction =
                        client_transaction.clone().try_into().map_err(internal_server_error)?;
                    Ok(TransactionWithHash {
                        transaction: starknet_api_transaction
                            .try_into()
                            .map_err(internal_server_error)?,
                        transaction_hash: client_transaction.transaction_hash(),
                    })
                })
                .collect::<Result<Vec<_>, ErrorObjectOwned>>()?;
            return Ok(Block {
                status: None,
                header,
                transactions: Transactions::Full(transactions),
            });
        }

        let block_number = get_accepted_block_number(&txn, block_id)?;
        let status = get_block_status(&txn, block_number)?;
        let header =
            GeneralBlockHeader::BlockHeader(get_block_header_by_number(&txn, block_number)?);
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

        Ok(Block {
            status: Some(status),
            header,
            transactions: Transactions::Full(transactions_with_hash),
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> RpcResult<Felt> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let maybe_pending_storage_diffs = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(
                read_pending_data(&self.pending_data, &txn)
                    .await?
                    .state_update
                    .state_diff
                    .storage_diffs,
            )
        } else {
            None
        };

        // Check that the block is valid and get the state number.
        let block_number = get_accepted_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let res = execution_utils::get_storage_at(
            &txn,
            state_number,
            maybe_pending_storage_diffs.as_ref(),
            contract_address,
            key,
        )
        .map_err(internal_server_error)?;

        // If the contract is not deployed, res will be 0. Checking if that's the case so that
        // we'll return an error instead.
        // Contract address 0x1 is a special address, it stores the block
        // hashes. Contracts are not deployed to this address.
        if res == Felt::default() && contract_address != *BLOCK_HASH_TABLE_ADDRESS {
            // check if the contract exists
            txn.get_state_reader()
                .map_err(internal_server_error)?
                .get_class_hash_at(state_number, &contract_address)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))?;
        }
        Ok(res)
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionWithHash> {
        verify_storage_scope(&self.storage_reader)?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        if let Some(transaction_index) =
            txn.get_transaction_idx_by_hash(&transaction_hash).map_err(internal_server_error)?
        {
            let transaction = txn
                .get_transaction(transaction_index)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;

            Ok(TransactionWithHash { transaction: transaction.try_into()?, transaction_hash })
        } else {
            // The transaction is not in any non-pending block. Search for it in the pending block
            // and if it's not found, return error.
            let client_transaction = read_pending_data(&self.pending_data, &txn)
                .await?
                .block
                .transactions
                .iter()
                .find(|transaction| transaction.transaction_hash() == transaction_hash)
                .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?
                .clone();

            let starknet_api_transaction: StarknetApiTransaction =
                client_transaction.try_into().map_err(internal_server_error)?;
            return Ok(TransactionWithHash {
                transaction: starknet_api_transaction.try_into()?,
                transaction_hash,
            });
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> RpcResult<TransactionWithHash> {
        verify_storage_scope(&self.storage_reader)?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let (starknet_api_transaction, transaction_hash) =
            if let BlockId::Tag(Tag::Pending) = block_id {
                let client_transaction = read_pending_data(&self.pending_data, &txn)
                    .await?
                    .block
                    .transactions
                    .get(index.0)
                    .ok_or_else(|| ErrorObjectOwned::from(INVALID_TRANSACTION_INDEX))?
                    .clone();
                let transaction_hash = client_transaction.transaction_hash();
                (client_transaction.try_into().map_err(internal_server_error)?, transaction_hash)
            } else {
                let block_number = get_accepted_block_number(&txn, block_id)?;

                let tx_index = TransactionIndex(block_number, index);
                let transaction = txn
                    .get_transaction(tx_index)
                    .map_err(internal_server_error)?
                    .ok_or_else(|| ErrorObjectOwned::from(INVALID_TRANSACTION_INDEX))?;
                let transaction_hash = txn
                    .get_transaction_hash_by_idx(&tx_index)
                    .map_err(internal_server_error)?
                    .ok_or_else(|| ErrorObjectOwned::from(INVALID_TRANSACTION_INDEX))?;
                (transaction, transaction_hash)
            };

        Ok(TransactionWithHash {
            transaction: starknet_api_transaction.try_into()?,
            transaction_hash,
        })
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_block_transaction_count(&self, block_id: BlockId) -> RpcResult<usize> {
        verify_storage_scope(&self.storage_reader)?;
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        if let BlockId::Tag(Tag::Pending) = block_id {
            let transactions_len =
                read_pending_data(&self.pending_data, &txn).await?.block.transactions.len();
            Ok(transactions_len)
        } else {
            let block_number = get_accepted_block_number(&txn, block_id)?;
            Ok(txn
                .get_block_transactions_count(block_number)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?)
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_state_update(&self, block_id: BlockId) -> RpcResult<StateUpdate> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        if let BlockId::Tag(Tag::Pending) = block_id {
            let state_update = read_pending_data(&self.pending_data, &txn).await?.state_update;
            return Ok(StateUpdate::PendingStateUpdate(PendingStateUpdate {
                old_root: state_update.old_root,
                state_diff: state_update.state_diff.into(),
            }));
        }

        // Get the block header for the block hash and state root.
        let block_number = get_accepted_block_number(&txn, block_id)?;
        let header: BlockHeader = get_block_header_by_number(&txn, block_number)?;

        // Get the old root.
        let old_root = match get_accepted_block_number(
            &txn,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.parent_hash)),
        ) {
            Ok(parent_block_number) => {
                get_block_header_by_number::<_, BlockHeader>(&txn, parent_block_number)?.new_root
            }
            Err(_) => GlobalRoot(Felt::try_from(GENESIS_HASH).map_err(internal_server_error)?),
        };

        // Get the block state diff.
        let mut thin_state_diff = txn
            .get_state_diff(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;
        // Remove empty storage diffs. Some blocks contain empty storage diffs that must be kept for
        // the computation of state diff commitment.
        thin_state_diff.storage_diffs.retain(|_k, v| !v.is_empty());

        Ok(StateUpdate::AcceptedStateUpdate(AcceptedStateUpdate {
            block_hash: header.block_hash,
            new_root: header.new_root,
            old_root,
            state_diff: thin_state_diff.into(),
        }))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<GeneralTransactionReceipt> {
        verify_storage_scope(&self.storage_reader)?;

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        if let Some(transaction_index) =
            txn.get_transaction_idx_by_hash(&transaction_hash).map_err(internal_server_error)?
        {
            let block_number = transaction_index.0;
            let status = get_block_status(&txn, block_number)?;

            // rejected blocks should not be a part of the API so we early return here.
            // this assumption also holds for the conversion from block status to transaction
            // finality status where we set rejected blocks to unreachable.
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

            Ok(GeneralTransactionReceipt::TransactionReceipt(TransactionReceipt {
                finality_status: status.into(),
                transaction_hash,
                block_hash,
                block_number,
                output,
            }))
        } else {
            // The transaction is not in any non-pending block. Search for it in the pending block
            // and if it's not found, return error.

            // TODO(shahak): Consider cloning the transactions and the receipts in order to free
            // the lock sooner (Check which is better).
            let pending_block = read_pending_data(&self.pending_data, &txn).await?.block;

            let client_transaction_receipt = pending_block
                .transaction_receipts
                .iter()
                .find(|receipt| receipt.transaction_hash == transaction_hash)
                .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?
                .clone();
            let client_transaction = &pending_block
                .transactions
                .iter()
                .find(|transaction| transaction.transaction_hash() == transaction_hash)
                .ok_or_else(|| ErrorObjectOwned::from(TRANSACTION_HASH_NOT_FOUND))?;
            let starknet_api_output =
                client_transaction_receipt.into_starknet_api_transaction_output(client_transaction);
            let output =
                PendingTransactionOutput::try_from(TransactionOutput::from(starknet_api_output))?;
            Ok(GeneralTransactionReceipt::PendingTransactionReceipt(PendingTransactionReceipt {
                // ACCEPTED_ON_L2 is the only finality status of a pending transaction.
                finality_status: PendingTransactionFinalityStatus::AcceptedOnL2,
                transaction_hash,
                output,
            }))
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> RpcResult<GatewayContractClass> {
        let block_id = if let BlockId::Tag(Tag::Pending) = block_id {
            let maybe_class = &self.pending_classes.read().await.get_class(class_hash);
            if let Some(class) = maybe_class {
                return class.clone().try_into().map_err(internal_server_error);
            } else {
                BlockId::Tag(Tag::Latest)
            }
        } else {
            block_id
        };

        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_accepted_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // The class might be a deprecated class. Search it first in the declared classes and if not
        // found, search in the deprecated classes.
        if let Some(class) = state_reader
            .get_class_definition_at(state_number, &class_hash)
            .map_err(internal_server_error)?
        {
            Ok(GatewayContractClass::Sierra(class.into()))
        } else {
            let class = state_reader
                .get_deprecated_class_definition_at(state_number, &class_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(CLASS_HASH_NOT_FOUND))?;
            Ok(GatewayContractClass::Cairo0(class.try_into().map_err(internal_server_error)?))
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<GatewayContractClass> {
        let class_hash = self.get_class_hash_at(block_id, contract_address).await?;
        self.get_class(block_id, class_hash).await
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<ClassHash> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let maybe_pending_deployed_contracts_and_replaced_classes =
            if let BlockId::Tag(Tag::Pending) = block_id {
                let pending_state_diff =
                    read_pending_data(&self.pending_data, &txn).await?.state_update.state_diff;
                Some((pending_state_diff.deployed_contracts, pending_state_diff.replaced_classes))
            } else {
                None
            };

        let block_number = get_accepted_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        execution_utils::get_class_hash_at(
            &txn,
            state_number,
            // This map converts &(T, S) to (&T, &S).
            maybe_pending_deployed_contracts_and_replaced_classes.as_ref().map(|t| (&t.0, &t.1)),
            contract_address,
        )
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_nonce(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<Nonce> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let maybe_pending_nonces = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(read_pending_data(&self.pending_data, &txn).await?.state_update.state_diff.nonces)
        } else {
            None
        };

        // Check that the block is valid and get the state number.
        let block_number = get_accepted_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        execution_utils::get_nonce_at(
            &txn,
            state_number,
            maybe_pending_nonces.as_ref(),
            contract_address,
        )
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(CONTRACT_NOT_FOUND))
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    fn chain_id(&self) -> RpcResult<String> {
        Ok(self.chain_id.as_hex())
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn get_events(&self, filter: EventFilter) -> RpcResult<EventsChunk> {
        verify_storage_scope(&self.storage_reader)?;

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
        let Some(latest_block_number) = get_latest_block_number(&txn)? else {
            if matches!(filter.to_block, Some(BlockId::Tag(Tag::Pending)) | None) {
                warn!(
                    "Received a request for pending events while there are no accepted blocks. \
                     This is currently unsupported. Returning no events."
                );
            }
            // There are no blocks.
            return Ok(EventsChunk { events: vec![], continuation_token: None });
        };
        let from_block_number = match filter.from_block {
            None => BlockNumber(0),
            Some(BlockId::Tag(Tag::Pending)) => latest_block_number.next(),
            Some(block_id) => get_accepted_block_number(&txn, block_id)?,
        };
        let mut to_block_number = match filter.to_block {
            Some(BlockId::Tag(Tag::Pending)) | None => latest_block_number.next(),
            Some(block_id) => get_accepted_block_number(&txn, block_id)?,
        };

        if from_block_number > to_block_number {
            return Ok(EventsChunk { events: vec![], continuation_token: None });
        }

        // Get the event index. If there's a continuation token we take the event index from there.
        // Otherwise, we take the first index in the from_block_number.
        let start_event_index = match &filter.continuation_token {
            Some(token) => token.parse()?.0,
            None => EventIndex(
                TransactionIndex(from_block_number, TransactionOffsetInBlock(0)),
                EventIndexInTransactionOutput(0),
            ),
        };

        let include_pending_block = to_block_number > latest_block_number;
        if include_pending_block {
            to_block_number = to_block_number.prev().expect(
                "A block number that's greater than another block number should have a predecessor",
            );
        }

        // Collect the requested events.
        // Once we collected enough events, we continue to check if there are any more events
        // corresponding to the requested filter. If there are, we return a continuation token
        // pointing to the next relevant event. Otherwise, we return a continuation token None.
        let mut filtered_events = vec![];
        if start_event_index.0.0 <= latest_block_number {
            for ((from_address, event_index), content) in txn
                .iter_events(filter.address, start_event_index, to_block_number)
                .map_err(internal_server_error)?
            {
                let block_number = (event_index.0).0;
                if block_number > to_block_number {
                    break;
                }
                if let Some(filter_address) = filter.address {
                    if from_address != filter_address {
                        // The iterator of this loop outputs only events that have the filter's
                        // address, unless there are no more such events and then it outputs other
                        // events, and we can stop the iteration.
                        break;
                    }
                }
                // TODO: Consider changing empty sets in the filer keys to None.
                if do_event_keys_match_filter(&content, &filter) {
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
                        block_hash: Some(header.block_hash),
                        block_number: Some(block_number),
                        transaction_hash,
                        event: starknet_api::transaction::Event { from_address, content },
                    };
                    filtered_events.push(emitted_event);
                }
            }
        }

        if include_pending_block {
            let pending_transaction_receipts =
                read_pending_data(&self.pending_data, &txn).await?.block.transaction_receipts;
            // Extract the first transaction offset and event offset from the starting EventIndex.
            let (transaction_start, event_start) = if start_event_index.0.0 > latest_block_number {
                (start_event_index.0.1.0, start_event_index.1.0)
            } else {
                (0, 0)
            };
            // TODO(shahak): Consider creating the iterator flattened and filtered.
            for (transaction_offset, receipt) in pending_transaction_receipts.iter().enumerate() {
                if transaction_offset < transaction_start {
                    continue;
                }
                for (event_offset, event) in receipt.events.iter().cloned().enumerate() {
                    if transaction_offset == transaction_start && event_offset < event_start {
                        continue;
                    }
                    if filtered_events.len() == filter.chunk_size {
                        return Ok(EventsChunk {
                            events: filtered_events,
                            continuation_token: Some(ContinuationToken::new(
                                ContinuationTokenAsStruct(EventIndex(
                                    TransactionIndex(
                                        latest_block_number.next(),
                                        TransactionOffsetInBlock(transaction_offset),
                                    ),
                                    EventIndexInTransactionOutput(event_offset),
                                )),
                            )?),
                        });
                    }
                    if !do_event_keys_match_filter(&event.content, &filter) {
                        continue;
                    }
                    if let Some(filter_address) = filter.address {
                        if event.from_address != filter_address {
                            continue;
                        }
                    }
                    filtered_events.push(Event {
                        block_hash: None,
                        block_number: None,
                        transaction_hash: receipt.transaction_hash,
                        event,
                    })
                }
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
    async fn call(&self, request: CallRequest, block_id: BlockId) -> RpcResult<Vec<Felt>> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let maybe_pending_data = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(client_pending_data_to_execution_pending_data(
                read_pending_data(&self.pending_data, &txn).await?,
                self.pending_classes.read().await.clone(),
            ))
        } else {
            None
        };
        let block_number = get_accepted_block_number(&txn, block_id)?;
        let block_not_reverted_validator = BlockNotRevertedValidator::new(block_number, &txn)?;
        drop(txn);
        let state_number = StateNumber::right_after_block(block_number);
        let block_execution_config = self
            .execution_config
            .get_execution_config_for_block(block_number)
            .map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?
            .clone();
        let chain_id = self.chain_id.clone();
        let reader = self.storage_reader.clone();
        let contract_address_copy = request.contract_address;

        let call_result = tokio::task::spawn_blocking(move || {
            execute_call(
                reader,
                maybe_pending_data,
                &chain_id,
                state_number,
                block_number,
                &contract_address_copy,
                request.entry_point_selector,
                request.calldata,
                &block_execution_config,
            )
        })
        .await
        .map_err(internal_server_error)?;

        block_not_reverted_validator.validate(&self.storage_reader)?;

        match call_result {
            Ok(res) => Ok(res.retdata.0),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err, ret)]
    async fn add_invoke_transaction(
        &self,
        invoke_transaction: TypedInvokeTransactionV1,
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
        deploy_account_transaction: TypedDeployAccountTransaction,
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
    async fn estimate_fee(
        &self,
        transactions: Vec<BroadcastedTransaction>,
        block_id: BlockId,
    ) -> RpcResult<Vec<FeeEstimate>> {
        trace!("Estimating fee of transactions: {:#?}", transactions);

        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let maybe_pending_data = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(client_pending_data_to_execution_pending_data(
                read_pending_data(&self.pending_data, &storage_txn).await?,
                self.pending_classes.read().await.clone(),
            ))
        } else {
            None
        };

        let executable_txns =
            transactions.into_iter().map(|tx| tx.try_into()).collect::<Result<_, _>>()?;

        let block_number = get_accepted_block_number(&storage_txn, block_id)?;
        let block_not_reverted_validator =
            BlockNotRevertedValidator::new(block_number, &storage_txn)?;
        drop(storage_txn);
        let state_number = StateNumber::right_after_block(block_number);
        let block_execution_config = self
            .execution_config
            .get_execution_config_for_block(block_number)
            .map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?
            .clone();
        let chain_id = self.chain_id.clone();
        let reader = self.storage_reader.clone();

        let estimate_fee_result = tokio::task::spawn_blocking(move || {
            exec_estimate_fee(
                executable_txns,
                &chain_id,
                reader,
                maybe_pending_data,
                state_number,
                block_number,
                &block_execution_config,
                false,
            )
        })
        .await
        .map_err(internal_server_error)?;

        block_not_reverted_validator.validate(&self.storage_reader)?;

        match estimate_fee_result {
            Ok(Ok(fees)) => Ok(fees
                .into_iter()
                .map(|(gas_price, fee, _)| FeeEstimate::from(gas_price, fee))
                .collect()),
            Ok(Err(_reverted_tx)) => Err(CONTRACT_ERROR.into()),
            Err(err) => Err(internal_server_error(err)),
        }
    }

    #[instrument(skip(self, transactions), level = "debug", err, ret)]
    async fn simulate_transactions(
        &self,
        block_id: BlockId,
        transactions: Vec<BroadcastedTransaction>,
        simulation_flags: Vec<SimulationFlag>,
    ) -> RpcResult<Vec<SimulatedTransaction>> {
        trace!("Simulating transactions: {:#?}", transactions);
        let executable_txns =
            transactions.into_iter().map(|tx| tx.try_into()).collect::<Result<_, _>>()?;

        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let maybe_pending_data = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(client_pending_data_to_execution_pending_data(
                read_pending_data(&self.pending_data, &storage_txn).await?,
                self.pending_classes.read().await.clone(),
            ))
        } else {
            None
        };

        let block_number = get_accepted_block_number(&storage_txn, block_id)?;
        let block_not_reverted_validator =
            BlockNotRevertedValidator::new(block_number, &storage_txn)?;
        drop(storage_txn);
        let state_number = StateNumber::right_after_block(block_number);
        let block_execution_config = self
            .execution_config
            .get_execution_config_for_block(block_number)
            .map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?
            .clone();
        let chain_id = self.chain_id.clone();
        let reader = self.storage_reader.clone();

        let charge_fee = !simulation_flags.contains(&SimulationFlag::SkipFeeCharge);
        let validate = !simulation_flags.contains(&SimulationFlag::SkipValidate);

        let simulate_transactions_result = tokio::task::spawn_blocking(move || {
            exec_simulate_transactions(
                executable_txns,
                None,
                &chain_id,
                reader,
                maybe_pending_data,
                state_number,
                block_number,
                &block_execution_config,
                charge_fee,
                validate,
            )
        })
        .await
        .map_err(internal_server_error)?;

        block_not_reverted_validator.validate(&self.storage_reader)?;

        match simulate_transactions_result {
            Ok(simulation_results) => Ok(simulation_results
                .into_iter()
                .map(|TransactionSimulationOutput { transaction_trace, gas_price, fee, .. }| {
                    SimulatedTransaction {
                        transaction_trace: transaction_trace.into(),
                        fee_estimation: FeeEstimate::from(gas_price, fee),
                    }
                })
                .collect()),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    async fn trace_transaction(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionTrace> {
        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let pending_block = read_pending_data(&self.pending_data, &storage_txn).await?.block;
        // Search for the transaction inside the pending block.
        let (
            maybe_pending_data,
            executable_transactions,
            transaction_hashes,
            block_number,
            state_number,
        ) = if let Some((pending_transaction_offset, _)) = pending_block
            .transaction_receipts
            .iter()
            .enumerate()
            .find(|(_, receipt)| receipt.transaction_hash == transaction_hash)
        {
            // If there are no blocks in the network and there is a pending block, as an edge
            // case we treat this as if the pending block is empty.
            let block_number =
                get_latest_block_number(&storage_txn)?.ok_or(INVALID_TRANSACTION_HASH)?;
            let state_number = StateNumber::right_after_block(block_number);
            let executable_transactions = pending_block
                .transactions
                .iter()
                .take(pending_transaction_offset + 1)
                .map(|client_transaction| {
                    let starknet_api_transaction: StarknetApiTransaction =
                        client_transaction.clone().try_into().map_err(internal_server_error)?;
                    stored_txn_to_executable_txn(
                        starknet_api_transaction,
                        &storage_txn,
                        state_number,
                    )
                })
                .collect::<Result<_, _>>()?;
            let transaction_hashes = pending_block
                .transaction_receipts
                .iter()
                .map(|receipt| receipt.transaction_hash)
                .collect();
            let maybe_pending_data = Some(ExecutionPendingData {
                timestamp: pending_block.timestamp,
                eth_l1_gas_price: pending_block.eth_l1_gas_price,
                sequencer: pending_block.sequencer_address,
                // The pending state diff should be empty since we look at the state in the
                // start of the pending block.
                ..Default::default()
            });
            (
                maybe_pending_data,
                executable_transactions,
                transaction_hashes,
                block_number,
                state_number,
            )
        } else {
            // Transaction is not inside the pending block. Search for it in the storage.
            let TransactionIndex(block_number, tx_offset) = storage_txn
                .get_transaction_idx_by_hash(&transaction_hash)
                .map_err(internal_server_error)?
                .ok_or(INVALID_TRANSACTION_HASH)?;

            let block_transactions = storage_txn
                .get_block_transactions(block_number)
                .map_err(internal_server_error)?
                .ok_or_else(|| {
                    internal_server_error(StorageError::DBInconsistency {
                        msg: format!("Missing block {block_number} transactions"),
                    })
                })?;

            let transaction_hashes = storage_txn
                .get_block_transaction_hashes(block_number)
                .map_err(internal_server_error)?
                .ok_or_else(|| {
                    internal_server_error(StorageError::DBInconsistency {
                        msg: format!("Missing block {block_number} transactions"),
                    })
                })?;

            let state_number = StateNumber::right_before_block(block_number);
            let executable_transactions = block_transactions
                .into_iter()
                .take(tx_offset.0 + 1)
                .map(|tx| stored_txn_to_executable_txn(tx, &storage_txn, state_number))
                .collect::<Result<_, _>>()?;

            (None, executable_transactions, transaction_hashes, block_number, state_number)
        };

        let block_not_reverted_validator =
            BlockNotRevertedValidator::new(block_number, &storage_txn)?;

        drop(storage_txn);

        let block_execution_config = self
            .execution_config
            .get_execution_config_for_block(block_number)
            .map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?
            .clone();
        let chain_id = self.chain_id.clone();
        let reader = self.storage_reader.clone();

        let simulate_transactions_result = tokio::task::spawn_blocking(move || {
            exec_simulate_transactions(
                executable_transactions,
                Some(transaction_hashes),
                &chain_id,
                reader,
                maybe_pending_data,
                state_number,
                block_number,
                &block_execution_config,
                true,
                true,
            )
        })
        .await
        .map_err(internal_server_error)?;

        block_not_reverted_validator.validate(&self.storage_reader)?;

        match simulate_transactions_result {
            Ok(mut simulation_results) => Ok(simulation_results
                .pop()
                .expect("Should have transaction exeuction result")
                .transaction_trace
                .into()),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self), level = "debug", err)]
    async fn trace_block_transactions(
        &self,
        block_id: BlockId,
    ) -> RpcResult<Vec<TransactionTraceWithHash>> {
        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let maybe_client_pending_data = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(read_pending_data(&self.pending_data, &storage_txn).await?)
        } else {
            None
        };

        let block_number = get_accepted_block_number(&storage_txn, block_id)?;

        let block_not_reverted_validator =
            BlockNotRevertedValidator::new(block_number, &storage_txn)?;

        let (maybe_pending_data, block_transactions, transaction_hashes, state_number) =
            match maybe_client_pending_data {
                Some(client_pending_data) => (
                    Some(ExecutionPendingData {
                        timestamp: client_pending_data.block.timestamp,
                        eth_l1_gas_price: client_pending_data.block.eth_l1_gas_price,
                        sequencer: client_pending_data.block.sequencer_address,
                        // The pending state diff should be empty since we look at the state in the
                        // start of the pending block.
                        ..Default::default()
                    }),
                    client_pending_data
                        .block
                        .transactions
                        .iter()
                        .map(|client_transaction| {
                            client_transaction.clone().try_into().map_err(internal_server_error)
                        })
                        .collect::<Result<Vec<_>, ErrorObjectOwned>>()?,
                    client_pending_data
                        .block
                        .transaction_receipts
                        .iter()
                        .map(|receipt| receipt.transaction_hash)
                        .collect(),
                    StateNumber::right_after_block(block_number),
                ),
                None => (
                    None,
                    storage_txn
                        .get_block_transactions(block_number)
                        .map_err(internal_server_error)?
                        .ok_or_else(|| {
                            internal_server_error(StorageError::DBInconsistency {
                                msg: format!("Missing block {block_number} transactions"),
                            })
                        })?,
                    storage_txn
                        .get_block_transaction_hashes(block_number)
                        .map_err(internal_server_error)?
                        .ok_or_else(|| {
                            internal_server_error(StorageError::DBInconsistency {
                                msg: format!("Missing block {block_number} transactions"),
                            })
                        })?,
                    StateNumber::right_before_block(block_number),
                ),
            };

        let executable_txns = block_transactions
            .into_iter()
            .map(|tx| stored_txn_to_executable_txn(tx, &storage_txn, state_number))
            .collect::<Result<_, _>>()?;

        drop(storage_txn);

        let block_execution_config = self
            .execution_config
            .get_execution_config_for_block(block_number)
            .map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?
            .clone();
        let chain_id = self.chain_id.clone();
        let reader = self.storage_reader.clone();
        let transaction_hashes_clone = transaction_hashes.clone();

        let simulate_transactions_result = tokio::task::spawn_blocking(move || {
            exec_simulate_transactions(
                executable_txns,
                Some(transaction_hashes_clone),
                &chain_id,
                reader,
                maybe_pending_data,
                state_number,
                block_number,
                &block_execution_config,
                true,
                true,
            )
        })
        .await
        .map_err(internal_server_error)?;

        block_not_reverted_validator.validate(&self.storage_reader)?;

        match simulate_transactions_result {
            Ok(simulation_results) => Ok(simulation_results
                .into_iter()
                .zip(transaction_hashes)
                .map(|(TransactionSimulationOutput { transaction_trace, .. }, transaction_hash)| {
                    TransactionTraceWithHash {
                        transaction_hash,
                        trace_root: transaction_trace.into(),
                    }
                })
                .collect()),
            Err(ExecutionError::StorageError(err)) => Err(internal_server_error(err)),
            Err(err) => Err(ErrorObjectOwned::from(JsonRpcError::try_from(err)?)),
        }
    }

    #[instrument(skip(self, message), level = "debug", err)]
    async fn estimate_message_fee(
        &self,
        message: MessageFromL1,
        block_id: BlockId,
    ) -> RpcResult<FeeEstimate> {
        trace!("Estimating fee of message: {:#?}", message);
        let storage_txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let maybe_pending_data = if let BlockId::Tag(Tag::Pending) = block_id {
            Some(client_pending_data_to_execution_pending_data(
                read_pending_data(&self.pending_data, &storage_txn).await?,
                self.pending_classes.read().await.clone(),
            ))
        } else {
            None
        };
        // Convert the message to an L1 handler transaction, and estimate the fee of the
        // transaction.
        // The fee input is used to bound the amount of fee used. Because we want to estimate the
        // fee, we pass u128::MAX so the execution won't fail.
        let executable_txns =
            vec![ExecutableTransactionInput::L1Handler(message.into(), Fee(u128::MAX), false)];

        let block_number = get_accepted_block_number(&storage_txn, block_id)?;
        let block_not_reverted_validator =
            BlockNotRevertedValidator::new(block_number, &storage_txn)?;
        drop(storage_txn);
        let state_number = StateNumber::right_after_block(block_number);
        let block_execution_config = self
            .execution_config
            .get_execution_config_for_block(block_number)
            .map_err(|err| {
                internal_server_error(format!("Failed to get execution config: {}", err))
            })?
            .clone();
        let chain_id = self.chain_id.clone();
        let reader = self.storage_reader.clone();

        let estimate_fee_result = tokio::task::spawn_blocking(move || {
            exec_estimate_fee(
                executable_txns,
                &chain_id,
                reader,
                maybe_pending_data,
                state_number,
                block_number,
                &block_execution_config,
                false,
            )
        })
        .await
        .map_err(internal_server_error)?;

        block_not_reverted_validator.validate(&self.storage_reader)?;

        match estimate_fee_result {
            Ok(Ok(fee_as_vec)) => {
                if fee_as_vec.len() != 1 {
                    return Err(internal_server_error(format!(
                        "Expected a single fee, got {}",
                        fee_as_vec.len()
                    )));
                }
                let (gas_price, fee, _unit) = fee_as_vec.first().expect("No fee was returned");
                Ok(FeeEstimate::from(*gas_price, *fee))
            }
            // Error in the execution of the contract.
            Ok(Err(_reverted_tx)) => Err(CONTRACT_ERROR.into()),
            // Internal error during the execution.
            Err(err) => Err(internal_server_error(err)),
        }
    }
}

async fn read_pending_data<Mode: TransactionKind>(
    pending_data: &Arc<RwLock<PendingData>>,
    txn: &StorageTxn<'_, Mode>,
) -> RpcResult<PendingData> {
    let latest_header: starknet_api::block::BlockHeader = match get_latest_block_number(txn)? {
        Some(latest_block_number) => get_block_header_by_number(txn, latest_block_number)?,
        None => starknet_api::block::BlockHeader {
            parent_hash: BlockHash(Felt::try_from(GENESIS_HASH).map_err(internal_server_error)?),
            ..Default::default()
        },
    };
    let pending_data = &pending_data.read().await;
    if pending_data.block.parent_block_hash == latest_header.block_hash {
        Ok((*pending_data).clone())
    } else {
        Ok(PendingData {
            block: PendingBlock {
                parent_block_hash: latest_header.block_hash,
                eth_l1_gas_price: latest_header.eth_l1_gas_price,
                strk_l1_gas_price: latest_header.strk_l1_gas_price,
                timestamp: latest_header.timestamp,
                sequencer_address: latest_header.sequencer,
                ..Default::default()
            },
            state_update: ClientPendingStateUpdate {
                old_root: latest_header.state_root,
                state_diff: Default::default(),
            },
        })
    }
}

fn do_event_keys_match_filter(event_content: &EventContent, filter: &EventFilter) -> bool {
    filter.keys.iter().enumerate().all(|(i, keys)| {
        event_content.keys.len() > i && (keys.is_empty() || keys.contains(&event_content.keys[i]))
    })
}

impl JsonRpcServerImpl for JsonRpcServerV0_4Impl {
    fn new(
        chain_id: ChainId,
        execution_config: ExecutionConfigByBlock,
        storage_reader: StorageReader,
        max_events_chunk_size: usize,
        max_events_keys: usize,
        starting_block: BlockHashAndNumber,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        pending_data: Arc<RwLock<PendingData>>,
        pending_classes: Arc<RwLock<PendingClasses>>,
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
            pending_data,
            pending_classes,
            writer_client,
        }
    }

    fn into_rpc_module(self) -> RpcModule<Self> {
        self.into_rpc()
    }
}
