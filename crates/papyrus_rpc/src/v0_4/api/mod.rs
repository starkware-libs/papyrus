use std::collections::HashSet;
use std::io::Read;

use flate2::bufread::GzDecoder;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_common::BlockHashAndNumber;
use papyrus_execution::{ExecutableTransactionInput, ExecutionError};
use papyrus_proc_macros::versioned_rpc;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::serialization::StorageSerdeError;
use papyrus_storage::db::RO;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageTxn;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::Program;
use starknet_api::state::{StateNumber, StorageKey};
use starknet_api::transaction::{EventKey, Fee, TransactionHash, TransactionOffsetInBlock};
use starknet_types_core::felt::Felt;
use tracing::debug;

use super::block::Block;
use super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedTransaction,
};
use super::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use super::error::{
    JsonRpcError,
    BLOCK_NOT_FOUND,
    CONTRACT_ERROR,
    CONTRACT_NOT_FOUND,
    INVALID_CONTINUATION_TOKEN,
};
use super::execution::TransactionTrace;
use super::state::{ContractClass, StateUpdate};
use super::transaction::{
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    Event,
    GeneralTransactionReceipt,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    MessageFromL1,
    TransactionWithHash,
    TypedDeployAccountTransaction,
    TypedInvokeTransactionV1,
};
use super::write_api_result::{AddDeclareOkResult, AddDeployAccountOkResult, AddInvokeOkResult};
use crate::api::{BlockId, CallRequest};
use crate::syncing_state::SyncingState;
use crate::{internal_server_error, ContinuationTokenAsStruct};

pub mod api_impl;
#[cfg(test)]
mod test;

#[versioned_rpc("V0_4")]
#[async_trait]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    fn block_number(&self) -> RpcResult<BlockNumber>;

    /// Gets the most recent accepted block hash and number.
    #[method(name = "blockHashAndNumber")]
    fn block_hash_and_number(&self) -> RpcResult<BlockHashAndNumber>;

    /// Gets block information with transaction hashes given a block identifier.
    #[method(name = "getBlockWithTxHashes")]
    async fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> RpcResult<Block>;

    /// Gets block information with full transactions given a block identifier.
    #[method(name = "getBlockWithTxs")]
    async fn get_block_w_full_transactions(&self, block_id: BlockId) -> RpcResult<Block>;

    /// Gets the value of the storage at the given address, key, and block.
    #[method(name = "getStorageAt")]
    async fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> RpcResult<Felt>;

    /// Gets the details of a submitted transaction.
    #[method(name = "getTransactionByHash")]
    async fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionWithHash>;

    /// Gets the details of a transaction by a given block id and index.
    #[method(name = "getTransactionByBlockIdAndIndex")]
    async fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> RpcResult<TransactionWithHash>;

    /// Gets the number of transactions in a block given a block id.
    #[method(name = "getBlockTransactionCount")]
    async fn get_block_transaction_count(&self, block_id: BlockId) -> RpcResult<usize>;

    /// Gets the information about the result of executing the requested block.
    #[method(name = "getStateUpdate")]
    async fn get_state_update(&self, block_id: BlockId) -> RpcResult<StateUpdate>;

    /// Gets the transaction receipt by the transaction hash.
    #[method(name = "getTransactionReceipt")]
    async fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<GeneralTransactionReceipt>;

    /// Gets the contract class definition associated with the given hash.
    #[method(name = "getClass")]
    async fn get_class(
        &self,
        block_id: BlockId,
        class_hash: ClassHash,
    ) -> RpcResult<GatewayContractClass>;

    /// Gets the contract class definition in the given block at the given address.
    #[method(name = "getClassAt")]
    async fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<GatewayContractClass>;

    /// Gets the contract class hash in the given block for the contract deployed at the given
    /// address.
    #[method(name = "getClassHashAt")]
    async fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<ClassHash>;

    /// Gets the nonce associated with the given address in the given block.
    #[method(name = "getNonce")]
    async fn get_nonce(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> RpcResult<Nonce>;

    /// Returns the currently configured StarkNet chain id.
    #[method(name = "chainId")]
    fn chain_id(&self) -> RpcResult<String>;

    /// Returns all events matching the given filter.
    #[method(name = "getEvents")]
    async fn get_events(&self, filter: EventFilter) -> RpcResult<EventsChunk>;

    /// Returns the synching status of the node, or false if the node is not synching.
    #[method(name = "syncing")]
    async fn syncing(&self) -> RpcResult<SyncingState>;

    /// Executes the entry point of the contract at the given address with the given calldata,
    /// returns the result (Retdata).
    #[method(name = "call")]
    async fn call(&self, request: CallRequest, block_id: BlockId) -> RpcResult<Vec<Felt>>;

    /// Submits a new invoke transaction to be added to the chain.
    #[method(name = "addInvokeTransaction")]
    async fn add_invoke_transaction(
        &self,
        invoke_transaction: TypedInvokeTransactionV1,
    ) -> RpcResult<AddInvokeOkResult>;

    /// Submits a new deploy account transaction to be added to the chain.
    #[method(name = "addDeployAccountTransaction")]
    async fn add_deploy_account_transaction(
        &self,
        deploy_account_transaction: TypedDeployAccountTransaction,
    ) -> RpcResult<AddDeployAccountOkResult>;

    /// Submits a new declare transaction to be added to the chain.
    #[method(name = "addDeclareTransaction")]
    async fn add_declare_transaction(
        &self,
        declare_transaction: BroadcastedDeclareTransaction,
    ) -> RpcResult<AddDeclareOkResult>;

    /// Estimates the fee of a series of transactions.
    #[method(name = "estimateFee")]
    async fn estimate_fee(
        &self,
        request: Vec<BroadcastedTransaction>,
        block_id: BlockId,
    ) -> RpcResult<Vec<FeeEstimate>>;

    /// Estimates the fee of a message from L1.
    #[method(name = "estimateMessageFee")]
    async fn estimate_message_fee(
        &self,
        message: MessageFromL1,
        block_id: BlockId,
    ) -> RpcResult<FeeEstimate>;

    /// Simulates execution of a series of transactions.
    #[method(name = "simulateTransactions")]
    async fn simulate_transactions(
        &self,
        block_id: BlockId,
        transactions: Vec<BroadcastedTransaction>,
        simulation_flags: Vec<SimulationFlag>,
    ) -> RpcResult<Vec<SimulatedTransaction>>;

    /// Calculates the transaction trace of a transaction that is already included in a block.
    #[method(name = "traceTransaction")]
    async fn trace_transaction(
        &self,
        transaction_hash: TransactionHash,
    ) -> RpcResult<TransactionTrace>;

    /// Calculates the transaction trace of all of the transactions in a block.
    #[method(name = "traceBlockTransactions")]
    async fn trace_block_transactions(
        &self,
        block_id: BlockId,
    ) -> RpcResult<Vec<TransactionTraceWithHash>>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(untagged)]
pub enum GatewayContractClass {
    Cairo0(DeprecatedContractClass),
    Sierra(ContractClass),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventsChunk {
    pub events: Vec<Event>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<ContinuationToken>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_block: Option<BlockId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_block: Option<BlockId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<ContinuationToken>,
    pub chunk_size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<ContractAddress>,
    #[serde(default)]
    pub keys: Vec<HashSet<EventKey>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct ContinuationToken(pub String);

impl ContinuationToken {
    fn parse(&self) -> Result<ContinuationTokenAsStruct, ErrorObjectOwned> {
        let ct = serde_json::from_str(&self.0)
            .map_err(|_| ErrorObjectOwned::from(INVALID_CONTINUATION_TOKEN))?;

        Ok(ContinuationTokenAsStruct(ct))
    }

    fn new(ct: ContinuationTokenAsStruct) -> Result<Self, ErrorObjectOwned> {
        Ok(Self(serde_json::to_string(&ct.0).map_err(internal_server_error)?))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct FeeEstimate {
    pub gas_consumed: Felt,
    pub gas_price: GasPrice,
    pub overall_fee: Fee,
}

impl FeeEstimate {
    pub fn from(gas_price: GasPrice, overall_fee: Fee) -> Self {
        match gas_price {
            GasPrice(0) => Self::default(),
            _ => {
                Self { gas_consumed: (overall_fee.0 / gas_price.0).into(), gas_price, overall_fee }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimulatedTransaction {
    pub transaction_trace: TransactionTrace,
    pub fee_estimation: FeeEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SimulationFlag {
    SkipValidate,
    SkipFeeCharge,
}

impl TryFrom<BroadcastedTransaction> for ExecutableTransactionInput {
    type Error = ErrorObjectOwned;
    fn try_from(value: BroadcastedTransaction) -> Result<Self, Self::Error> {
        match value {
            BroadcastedTransaction::Declare(tx) => Ok(tx.try_into()?),
            BroadcastedTransaction::DeployAccount(tx) => Ok(Self::DeployAccount(tx.into(), false)),
            BroadcastedTransaction::Invoke(tx) => Ok(Self::Invoke(tx.into(), false)),
        }
    }
}

pub(crate) fn stored_txn_to_executable_txn(
    stored_txn: starknet_api::transaction::Transaction,
    storage_txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
) -> Result<ExecutableTransactionInput, ErrorObjectOwned> {
    match stored_txn {
        starknet_api::transaction::Transaction::Declare(
            starknet_api::transaction::DeclareTransaction::V0(value),
        ) => {
            // Copy the class hash before the value moves.
            let class_hash = value.class_hash;
            Ok(ExecutableTransactionInput::DeclareV0(
                value,
                get_deprecated_class_for_re_execution(storage_txn, state_number, class_hash)?,
                false,
            ))
        }
        starknet_api::transaction::Transaction::Declare(
            starknet_api::transaction::DeclareTransaction::V1(value),
        ) => {
            // Copy the class hash before the value moves.
            let class_hash = value.class_hash;
            Ok(ExecutableTransactionInput::DeclareV1(
                value,
                get_deprecated_class_for_re_execution(storage_txn, state_number, class_hash)?,
                false,
            ))
        }
        starknet_api::transaction::Transaction::Declare(
            starknet_api::transaction::DeclareTransaction::V2(value),
        ) => {
            let casm = storage_txn
                .get_casm(&value.class_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| {
                    internal_server_error(format!(
                        "Missing casm of class hash {}.",
                        value.class_hash
                    ))
                })?;
            Ok(ExecutableTransactionInput::DeclareV2(value, casm, false))
        }
        starknet_api::transaction::Transaction::Declare(
            starknet_api::transaction::DeclareTransaction::V3(value),
        ) => {
            let casm = storage_txn
                .get_casm(&value.class_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| {
                    internal_server_error(format!(
                        "Missing casm of class hash {}.",
                        value.class_hash
                    ))
                })?;
            Ok(ExecutableTransactionInput::DeclareV3(value, casm, false))
        }
        starknet_api::transaction::Transaction::Deploy(_) => {
            Err(internal_server_error("Deploy txns not supported in execution"))
        }
        starknet_api::transaction::Transaction::DeployAccount(deploy_account_tx) => {
            Ok(ExecutableTransactionInput::DeployAccount(deploy_account_tx, false))
        }
        starknet_api::transaction::Transaction::Invoke(value) => {
            Ok(ExecutableTransactionInput::Invoke(value, false))
        }
        starknet_api::transaction::Transaction::L1Handler(value) => {
            // todo(yair): This is a temporary solution until we have a better way to get the l1
            // fee.
            let paid_fee_on_l1 = Fee(1);
            Ok(ExecutableTransactionInput::L1Handler(value, paid_fee_on_l1, false))
        }
    }
}

// For re-execution (traceTransaction, traceBlockTransactions) we need to get the class definition
// of declare transactions from the storage before the execution. They are stored in the state after
// the block in which they appeared, so we need to get it from the state after given block.
fn get_deprecated_class_for_re_execution(
    storage_txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
    class_hash: ClassHash,
) -> Result<starknet_api::deprecated_contract_class::ContractClass, ErrorObjectOwned> {
    let state_number_after_block = StateNumber::right_after_block(state_number.block_after());
    storage_txn
        .get_state_reader()
        .map_err(internal_server_error)?
        .get_deprecated_class_definition_at(state_number_after_block, &class_hash)
        .map_err(internal_server_error)?
        .ok_or_else(|| {
            internal_server_error(format!("Missing deprecated class definition of {class_hash}."))
        })
}

impl TryFrom<BroadcastedDeclareTransaction> for ExecutableTransactionInput {
    type Error = ErrorObjectOwned;
    fn try_from(value: BroadcastedDeclareTransaction) -> Result<Self, Self::Error> {
        match value {
            BroadcastedDeclareTransaction::V1(BroadcastedDeclareV1Transaction {
                r#type: _,
                contract_class,
                sender_address,
                nonce,
                max_fee,
                signature,
            }) => Ok(Self::DeclareV1(
                starknet_api::transaction::DeclareTransactionV0V1 {
                    max_fee,
                    signature,
                    nonce,
                    // The blockifier doesn't need the class hash, but it uses the SN_API
                    // DeclareTransactionV0V1 which requires it.
                    class_hash: ClassHash::default(),
                    sender_address,
                },
                user_deprecated_contract_class_to_sn_api(contract_class)?,
                false,
            )),
            BroadcastedDeclareTransaction::V2(_) => {
                // TODO(yair): We need a way to get the casm of a declare V2 transaction.
                Err(internal_server_error("Declare V2 is not supported yet in execution."))
            }
        }
    }
}

fn user_deprecated_contract_class_to_sn_api(
    value: starknet_client::writer::objects::transaction::DeprecatedContractClass,
) -> Result<starknet_api::deprecated_contract_class::ContractClass, ErrorObjectOwned> {
    Ok(starknet_api::deprecated_contract_class::ContractClass {
        abi: value.abi,
        program: decompress_program(&value.compressed_program)?,
        entry_points_by_type: value.entry_points_by_type,
    })
}

impl From<DeployAccountTransaction> for starknet_api::transaction::DeployAccountTransaction {
    fn from(tx: DeployAccountTransaction) -> Self {
        match tx {
            DeployAccountTransaction::Version1(DeployAccountTransactionV1 {
                max_fee,
                signature,
                nonce,
                class_hash,
                contract_address_salt,
                constructor_calldata,
                version: _,
            }) => Self::V1(starknet_api::transaction::DeployAccountTransactionV1 {
                max_fee,
                signature,
                nonce,
                class_hash,
                contract_address_salt,
                constructor_calldata,
            }),
        }
    }
}

impl From<InvokeTransaction> for starknet_api::transaction::InvokeTransaction {
    fn from(value: InvokeTransaction) -> Self {
        match value {
            InvokeTransaction::Version0(InvokeTransactionV0 {
                max_fee,
                version: _,
                signature,
                contract_address,
                entry_point_selector,
                calldata,
            }) => Self::V0(starknet_api::transaction::InvokeTransactionV0 {
                max_fee,
                signature,
                contract_address,
                entry_point_selector,
                calldata,
            }),
            InvokeTransaction::Version1(InvokeTransactionV1 {
                max_fee,
                version: _,
                signature,
                nonce,
                sender_address,
                calldata,
            }) => Self::V1(starknet_api::transaction::InvokeTransactionV1 {
                max_fee,
                signature,
                nonce,
                sender_address,
                calldata,
            }),
        }
    }
}

impl TryFrom<ApiContractClass> for GatewayContractClass {
    type Error = StorageSerdeError;
    fn try_from(class: ApiContractClass) -> Result<Self, Self::Error> {
        match class {
            ApiContractClass::DeprecatedContractClass(deprecated_class) => {
                Ok(Self::Cairo0(deprecated_class.try_into()?))
            }
            ApiContractClass::ContractClass(sierra_class) => Ok(Self::Sierra(sierra_class.into())),
        }
    }
}

impl TryFrom<ExecutionError> for JsonRpcError {
    type Error = ErrorObjectOwned;
    fn try_from(value: ExecutionError) -> Result<Self, Self::Error> {
        match value {
            ExecutionError::MissingCompiledClass { class_hash } => {
                debug!(
                    "Execution failed because it required the compiled class with hash \
                     {class_hash} and we didn't download it yet."
                );
                Ok(BLOCK_NOT_FOUND)
            }
            ExecutionError::ContractNotFound { .. } => Ok(CONTRACT_NOT_FOUND),
            // All other execution errors are considered contract errors.
            _ => Ok(CONTRACT_ERROR),
        }
    }
}

pub(crate) fn decompress_program(
    base64_compressed_program: &String,
) -> Result<Program, ErrorObjectOwned> {
    base64::decode(base64_compressed_program).map_err(internal_server_error)?;
    let compressed_data =
        base64::decode(base64_compressed_program).map_err(internal_server_error)?;
    let mut decoder = GzDecoder::new(compressed_data.as_slice());
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).map_err(internal_server_error)?;
    serde_json::from_reader(decompressed.as_slice()).map_err(internal_server_error)
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct TransactionTraceWithHash {
    pub transaction_hash: TransactionHash,
    pub trace_root: TransactionTrace,
}
