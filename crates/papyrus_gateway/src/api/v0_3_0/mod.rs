use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use papyrus_proc_macros::versioned_rpc;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{TransactionHash, TransactionOffsetInBlock};

use super::{BlockHashAndNumber, BlockId, EventFilter, EventsChunk, GatewayContractClass};
use crate::block::Block;
use crate::state::StateUpdate;
use crate::transaction::{TransactionReceiptWithStatus, TransactionWithType};

pub mod v0_3_0_impl;
#[cfg(test)]
mod v0_3_0_test;

#[versioned_rpc("V0_3_0")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    fn block_number(&self) -> Result<BlockNumber, Error>;

    /// Gets the most recent accepted block hash and number.
    #[method(name = "blockHashAndNumber")]
    fn block_hash_and_number(&self) -> Result<BlockHashAndNumber, Error>;

    /// Gets block information with transaction hashes given a block identifier.
    #[method(name = "getBlockWithTxHashes")]
    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> Result<Block, Error>;

    /// Gets block information with full transactions given a block identifier.
    #[method(name = "getBlockWithTxs")]
    fn get_block_w_full_transactions(&self, block_id: BlockId) -> Result<Block, Error>;

    /// Gets the value of the storage at the given address, key, and block.
    #[method(name = "getStorageAt")]
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> Result<StarkFelt, Error>;

    /// Gets the details of a submitted transaction.
    #[method(name = "getTransactionByHash")]
    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionWithType, Error>;

    /// Gets the details of a transaction by a given block id and index.
    #[method(name = "getTransactionByBlockIdAndIndex")]
    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> Result<TransactionWithType, Error>;

    /// Gets the number of transactions in a block given a block id.
    #[method(name = "getBlockTransactionCount")]
    fn get_block_transaction_count(&self, block_id: BlockId) -> Result<usize, Error>;

    /// Gets the information about the result of executing the requested block.
    #[method(name = "getStateUpdate")]
    fn get_state_update(&self, block_id: BlockId) -> Result<StateUpdate, Error>;

    /// Gets the transaction receipt by the transaction hash.
    #[method(name = "getTransactionReceipt")]
    fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionReceiptWithStatus, Error>;

    /// Gets the contract class definition associated with the given hash.
    #[method(name = "getClass")]
    fn get_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> Result<GatewayContractClass, Error>;

    /// Gets the contract class definition in the given block at the given address.
    #[method(name = "getClassAt")]
    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<GatewayContractClass, Error>;

    /// Gets the contract class hash in the given block for the contract deployed at the given
    /// address.
    #[method(name = "getClassHashAt")]
    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, Error>;

    /// Gets the nonce associated with the given address in the given block.
    #[method(name = "getNonce")]
    fn get_nonce(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<Nonce, Error>;

    /// Returns the currently configured StarkNet chain id.
    #[method(name = "chainId")]
    fn chain_id(&self) -> Result<String, Error>;

    /// Returns all events matching the given filter.
    #[method(name = "getEvents")]
    fn get_events(&self, filter: EventFilter) -> Result<EventsChunk, Error>;
}
