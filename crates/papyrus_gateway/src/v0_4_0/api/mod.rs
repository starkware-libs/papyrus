use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use papyrus_common::SyncingState;
use papyrus_proc_macros::versioned_rpc;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{Calldata, TransactionHash, TransactionOffsetInBlock};

use super::block::Block;
use super::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use super::state::{ContractClass, StateUpdate};
use super::transaction::{Event, TransactionReceiptWithStatus, TransactionWithHash};
use crate::api::{BlockHashAndNumber, BlockId, ContinuationToken, EventFilter};

pub mod api_impl;
#[cfg(test)]
mod test;

#[versioned_rpc("V0_4_0")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    fn block_number(&self) -> RpcResult<BlockNumber>;

    /// Gets the most recent accepted block hash and number.
    #[method(name = "blockHashAndNumber")]
    fn block_hash_and_number(&self) -> RpcResult<BlockHashAndNumber>;

    /// Gets block information with transaction hashes given a block identifier.
    #[method(name = "getBlockWithTxHashes")]
    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> RpcResult<Block>;

    /// Gets block information with full transactions given a block identifier.
    #[method(name = "getBlockWithTxs")]
    fn get_block_w_full_transactions(&self, block_id: BlockId) -> RpcResult<Block>;

    /// Gets the value of the storage at the given address, key, and block.
    #[method(name = "getStorageAt")]
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> RpcResult<StarkFelt>;

    /// Gets the details of a submitted transaction.
    #[method(name = "getTransactionByHash")]
    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionWithHash>;

    /// Gets the details of a transaction by a given block id and index.
    #[method(name = "getTransactionByBlockIdAndIndex")]
    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> RpcResult<TransactionWithHash>;

    /// Gets the number of transactions in a block given a block id.
    #[method(name = "getBlockTransactionCount")]
    fn get_block_transaction_count(&self, block_id: BlockId) -> RpcResult<usize>;

    /// Gets the information about the result of executing the requested block.
    #[method(name = "getStateUpdate")]
    fn get_state_update(&self, block_id: BlockId) -> RpcResult<StateUpdate>;

    /// Gets the transaction receipt by the transaction hash.
    #[method(name = "getTransactionReceipt")]
    fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionReceiptWithStatus>;

    /// Gets the contract class definition associated with the given hash.
    #[method(name = "getClass")]
    fn get_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> RpcResult<GatewayContractClass>;

    /// Gets the contract class definition in the given block at the given address.
    #[method(name = "getClassAt")]
    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<GatewayContractClass>;

    /// Gets the contract class hash in the given block for the contract deployed at the given
    /// address.
    #[method(name = "getClassHashAt")]
    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<ClassHash>;

    /// Gets the nonce associated with the given address in the given block.
    #[method(name = "getNonce")]
    fn get_nonce(&self, block_id: BlockId, contract_address: ContractAddress) -> RpcResult<Nonce>;

    /// Returns the currently configured StarkNet chain id.
    #[method(name = "chainId")]
    fn chain_id(&self) -> RpcResult<String>;

    /// Returns all events matching the given filter.
    #[method(name = "getEvents")]
    fn get_events(&self, filter: EventFilter) -> RpcResult<EventsChunk>;

    /// Returns the synching status of the node, or false if the node is not synching.
    #[method(name = "syncing")]
    fn syncing(&self) -> RpcResult<SyncingState>;

    /// Executes the entry point of the contract at the given address with the given calldata,
    /// returns the result (Retdata).
    #[method(name = "call")]
    fn call(
        &self,
        contract_address: ContractAddress,
        entry_point_selector: EntryPointSelector,
        calldata: Calldata,
        block_id: BlockId,
    ) -> RpcResult<Vec<StarkFelt>>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GatewayContractClass {
    Cairo0(DeprecatedContractClass),
    Sierra(ContractClass),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventsChunk {
    pub events: Vec<Event>,
    pub continuation_token: Option<ContinuationToken>,
}
