#![warn(missing_docs)]
//! Functionality for executing Starknet transactions and contract entry points.
#[cfg(test)]
mod execution_test;
pub mod execution_utils;
mod state_reader;

#[cfg(test)]
mod test_utils;
#[cfg(any(feature = "testing", test))]
pub mod testing_instances;

pub mod objects;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::{fs, iter};

use blockifier::block_context::BlockContext;
use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::execution::entry_point::{
    CallEntryPoint,
    CallExecution,
    CallType as BlockifierCallType,
    EntryPointExecutionContext,
    ExecutionResources,
};
use blockifier::execution::errors::{EntryPointExecutionError, PreExecutionError};
use blockifier::state::cached_state::CachedState;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::transaction::objects::{AccountTransactionContext, TransactionExecutionInfo};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::transaction::transactions::ExecutableTransaction;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use cairo_vm::types::errors::program_errors::ProgramError;
use objects::TransactionTrace;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageTxn};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector};
// TODO: merge multiple EntryPointType structs in SN_API into one.
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointType,
};
use starknet_api::state::StateNumber;
use starknet_api::transaction::{
    Calldata,
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    Fee,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction,
    TransactionHash,
};
use state_reader::ExecutionStateReader;

/// Result type for execution functions.
pub type ExecutionResult<T> = Result<T, ExecutionError>;

/// The path to the default execution config file.
pub const DEFAULT_CONFIG_FILE: &str = "default_config.json";

/// Returns the absolute path of the execution config file.
pub fn get_config_file(file_name: &str) -> File {
    // TODO(Omri): Find a better way to get the config file path.
    let home_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(s) => format!("{}/../..", s),
        _ => "".to_string(),
    };
    let file_full_name = Path::new(&home_dir).join("config/execution_config").join(file_name);
    fs::File::open(file_full_name).unwrap()
}

trait ExecutionConfigV0_12_1 {
    fn fee_contract_address(&self) -> ContractAddress;
    fn invoke_tx_max_n_steps(&self) -> u32;
    fn validate_tx_max_n_steps(&self) -> u32;
    fn max_recursion_depth(&self) -> usize;
    fn step_gas_cost(&self) -> u64;
    fn initial_gas_cost(&self) -> u64;
    fn n_steps(&self) -> f64; // N_STEPS_RESOURCE
    fn pedersen_builtin(&self) -> f64; // HASH_BUILTIN_NAME
    fn range_check_builtin(&self) -> f64; // RANGE_CHECK_BUILTIN_NAME
    fn ecdsa_builtin(&self) -> f64; // SIGNATURE_BUILTIN_NAME
    fn bitwise_builtin(&self) -> f64; // BITWISE_BUILTIN_NAME
    fn poseidon_builtin(&self) -> f64; // POSEIDON_BUILTIN_NAME
    fn output_builtin(&self) -> f64; // OUTPUT_BUILTIN_NAME
    fn ec_op_builtin(&self) -> f64; // EC_OP_BUILTIN_NAME
    fn keccak_builtin(&self) -> f64; // KECCAK_BUILTIN_NAME
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(missing_docs)]
/// Parameters that are needed for execution.
// TODO(yair): Find a way to get them from the Starknet general config.
pub struct BlockExecutionConfig {
    parameters: BTreeMap<String, serde_json::Value>,
}

impl ExecutionConfigV0_12_1 for BlockExecutionConfig {
    fn fee_contract_address(&self) -> ContractAddress {
        serde_json::from_value(self.parameters.get("fee_contract_address").unwrap().clone())
            .unwrap()
    }
    fn invoke_tx_max_n_steps(&self) -> u32 {
        serde_json::from_value(self.parameters.get("invoke_tx_max_n_steps").unwrap().clone())
            .unwrap()
    }
    fn validate_tx_max_n_steps(&self) -> u32 {
        serde_json::from_value(self.parameters.get("validate_tx_max_n_steps").unwrap().clone())
            .unwrap()
    }
    fn max_recursion_depth(&self) -> usize {
        serde_json::from_value(self.parameters.get("max_recursion_depth").unwrap().clone()).unwrap()
    }
    fn step_gas_cost(&self) -> u64 {
        serde_json::from_value(self.parameters.get("step_gas_cost").unwrap().clone()).unwrap()
    }
    fn initial_gas_cost(&self) -> u64 {
        serde_json::from_value(self.parameters.get("initial_gas_cost").unwrap().clone()).unwrap()
    }
    fn n_steps(&self) -> f64 {
        serde_json::from_value(self.parameters.get("n_steps").unwrap().clone()).unwrap()
    }
    fn pedersen_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("pedersen_builtin").unwrap().clone()).unwrap()
    }
    fn range_check_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("range_check_builtin").unwrap().clone()).unwrap()
    }
    fn ecdsa_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("ecdsa_builtin").unwrap().clone()).unwrap()
    }
    fn bitwise_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("bitwise_builtin").unwrap().clone()).unwrap()
    }
    fn poseidon_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("poseidon_builtin").unwrap().clone()).unwrap()
    }
    fn output_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("output_builtin").unwrap().clone()).unwrap()
    }
    fn ec_op_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("ec_op_builtin").unwrap().clone()).unwrap()
    }
    fn keccak_builtin(&self) -> f64 {
        serde_json::from_value(self.parameters.get("keccak_builtin").unwrap().clone()).unwrap()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct ExecutionConfigByBlock {
    pub execution_config_segments: BTreeMap<BlockNumber, BlockExecutionConfig>,
}

impl Default for ExecutionConfigByBlock {
    fn default() -> Self {
        let default_config_file = get_config_file(DEFAULT_CONFIG_FILE);
        serde_json::from_reader(default_config_file).unwrap()
    }
}

impl ExecutionConfigByBlock {
    /// Returns the execution config for a given block number.
    pub fn get_execution_config_for_block(
        &self,
        block_number: BlockNumber,
    ) -> ExecutionResult<&BlockExecutionConfig> {
        let segments = &self.execution_config_segments;
        if segments.is_empty() || segments.keys().min() != Some(&BlockNumber(0)) {
            return Err(ExecutionError::ConfigError);
        }

        // TODO(yael): use the upper_bound feature once stable
        // Ok(segments.upper_bound(std::ops::Bound::Included(&block_number)).value().unwrap().
        // clone())

        for (segment_block_number, segment) in segments.iter() {
            if block_number < *segment_block_number {
                return Ok(segment);
            }
        }
        return Ok(segments.last_key_value().unwrap().1);
    }
}

/// Returns the execution config from the config file.
impl TryFrom<ExecutionConfig> for ExecutionConfigByBlock {
    type Error = serde_json::Error;

    fn try_from(execution_config: ExecutionConfig) -> Result<Self, Self::Error> {
        let config_file = get_config_file(&execution_config.config_file_name);
        serde_json::from_reader(config_file)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct ExecutionConfig {
    pub config_file_name: String,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        ExecutionConfig { config_file_name: DEFAULT_CONFIG_FILE.to_string() }
    }
}

impl SerializeConfig for ExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "config_file_name",
            &self.config_file_name,
            "Name of the ExecutionConfig configuration file.",
        )])
    }
}

impl BlockExecutionConfig {
    /// Returns the VM resources fee cost as a map from resource name to cost.
    pub fn vm_resources_fee_cost(&self) -> Arc<HashMap<String, f64>> {
        Arc::new(HashMap::from([
            ("n_steps".to_string(), self.n_steps()),
            ("pedersen_builtin".to_string(), self.pedersen_builtin()),
            ("range_check_builtin".to_string(), self.range_check_builtin()),
            ("ecdsa_builtin".to_string(), self.ecdsa_builtin()),
            ("bitwise_builtin".to_string(), self.bitwise_builtin()),
            ("poseidon_builtin".to_string(), self.poseidon_builtin()),
            ("output_builtin".to_string(), self.output_builtin()),
            ("ec_op_builtin".to_string(), self.ec_op_builtin()),
            ("keccak_builtin".to_string(), self.keccak_builtin()),
        ]))
    }
}

#[allow(missing_docs)]
// TODO(yair): arrange the errors into a normal error type.
/// The error type for the execution module.
#[derive(thiserror::Error, Debug)]
pub enum ExecutionError {
    #[error(
        "The contract at address {contract_address:?} is not found at state number \
         {state_number:?}."
    )]
    ContractNotFound { contract_address: ContractAddress, state_number: StateNumber },
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(
        "The node is not synced. state_number: {state_number:?}, compiled_class_marker: \
         {compiled_class_marker:?}"
    )]
    NotSynced { state_number: StateNumber, compiled_class_marker: BlockNumber },
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    PreExecutionError(#[from] PreExecutionError),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error("Charging fee is not supported yet in execution.")]
    ChargeFeeNotSupported,
    #[error("Config does not contain the default configuration")]
    ConfigError,
}

/// Executes a StarkNet call and returns the execution result.
pub fn execute_call(
    txn: &StorageTxn<'_, RO>,
    chain_id: &ChainId,
    state_number: StateNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
    execution_config: &BlockExecutionConfig,
) -> ExecutionResult<CallExecution> {
    verify_node_synced(txn, state_number)?;
    verify_contract_exists(contract_address, txn, state_number)?;

    let call_entry_point = CallEntryPoint {
        class_hash: None,
        code_address: Some(*contract_address),
        entry_point_type: EntryPointType::External,
        entry_point_selector,
        calldata,
        storage_address: *contract_address,
        caller_address: ContractAddress::default(),
        call_type: BlockifierCallType::Call,
        // TODO(yair): check if this is the correct value.
        initial_gas: execution_config.initial_gas_cost(),
    };
    let mut cached_state = CachedState::from(ExecutionStateReader { txn, state_number });
    let header =
        txn.get_block_header(state_number.block_after())?.expect("Should have block header.");
    let block_context = create_block_context(
        chain_id.clone(),
        header.block_number,
        header.timestamp,
        header.gas_price,
        &header.sequencer,
        execution_config,
    );
    let mut context = EntryPointExecutionContext::new(
        block_context,
        AccountTransactionContext::default(),
        execution_config.invoke_tx_max_n_steps() as usize,
    );

    let res = call_entry_point.execute(
        &mut cached_state,
        &mut ExecutionResources::default(),
        &mut context,
    )?;

    Ok(res.execution)
}

fn verify_node_synced(txn: &StorageTxn<'_, RO>, state_number: StateNumber) -> ExecutionResult<()> {
    let compiled_class_marker = txn.get_compiled_class_marker()?;
    let synced_up_to = StateNumber::right_before_block(compiled_class_marker);
    if state_number >= synced_up_to {
        return Err(ExecutionError::NotSynced { state_number, compiled_class_marker });
    }

    Ok(())
}

fn verify_contract_exists(
    contract_address: &ContractAddress,
    txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
) -> ExecutionResult<()> {
    txn.get_state_reader()?.get_class_hash_at(state_number, contract_address)?.ok_or(
        ExecutionError::ContractNotFound { contract_address: *contract_address, state_number },
    )?;
    Ok(())
}

fn create_block_context(
    chain_id: ChainId,
    block_number: BlockNumber,
    block_timestamp: BlockTimestamp,
    gas_price: GasPrice,
    sequencer_address: &ContractAddress,
    execution_config: &BlockExecutionConfig,
) -> BlockContext {
    BlockContext {
        chain_id,
        block_number,
        block_timestamp,
        sequencer_address: *sequencer_address,
        fee_token_address: execution_config.fee_contract_address(),
        vm_resource_fee_cost: execution_config.vm_resources_fee_cost(),
        invoke_tx_max_n_steps: execution_config.invoke_tx_max_n_steps(),
        validate_max_n_steps: execution_config.validate_tx_max_n_steps(),
        max_recursion_depth: execution_config.max_recursion_depth(),
        gas_price: gas_price.0,
    }
}

/// The transaction input to be executed.
// TODO(yair): This should use broadcasted transactions instead of regular transactions, but the
// blockifier expects regular transactions. Consider changing the blockifier to use broadcasted txs.
#[allow(missing_docs)]
#[derive(Clone, Debug)]
pub enum ExecutableTransactionInput {
    Invoke(InvokeTransaction),
    // todo(yair): Do we need to support V0?
    DeclareV0(DeclareTransactionV0V1, DeprecatedContractClass),
    DeclareV1(DeclareTransactionV0V1, DeprecatedContractClass),
    DeclareV2(DeclareTransactionV2, CasmContractClass),
    Deploy(DeployAccountTransaction),
    L1Handler(L1HandlerTransaction, Fee),
}

/// Returns the fee estimation for a series of transactions.
pub fn estimate_fee(
    txs: Vec<ExecutableTransactionInput>,
    chain_id: &ChainId,
    storage_txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
    execution_config: &BlockExecutionConfig,
) -> ExecutionResult<Vec<(GasPrice, Fee)>> {
    let (txs_execution_info, block_context) = execute_transactions(
        txs,
        None,
        chain_id,
        storage_txn,
        state_number,
        execution_config,
        false,
        false,
    )?;
    Ok(txs_execution_info
        .into_iter()
        .map(|tx_execution_info| (GasPrice(block_context.gas_price), tx_execution_info.actual_fee))
        .collect())
}

// Executes a series of transactions and returns the execution results.
#[allow(clippy::too_many_arguments)]
fn execute_transactions(
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    chain_id: &ChainId,
    storage_txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
    execution_config: &BlockExecutionConfig,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<(Vec<TransactionExecutionInfo>, BlockContext)> {
    verify_node_synced(storage_txn, state_number)?;

    // TODO(yair): When we support pending blocks, use the latest block header instead of the
    // pending block header.

    // Create the block context from the block in which the transactions should run.
    let header = storage_txn
        .get_block_header(state_number.block_after())?
        .expect("Should have block header.");

    // The starknet state will be from right before the block in which the transactions should run.
    let mut cached_state =
        CachedState::from(ExecutionStateReader { txn: storage_txn, state_number });
    let block_context = create_block_context(
        chain_id.clone(),
        header.block_number,
        header.timestamp,
        header.gas_price,
        &header.sequencer,
        execution_config,
    );

    let tx_hashes_iter: Box<dyn Iterator<Item = Option<TransactionHash>>> = match tx_hashes {
        Some(hashes) => Box::new(hashes.into_iter().map(Some)),
        None => Box::new(iter::repeat(None)),
    };

    let mut res = vec![];
    for (tx, tx_hash) in txs.into_iter().zip(tx_hashes_iter) {
        let blockifier_tx = to_blockifier_tx(tx, tx_hash)?;
        let tx_execution_info =
            blockifier_tx.execute(&mut cached_state, &block_context, charge_fee, validate)?;
        res.push(tx_execution_info);
    }

    Ok((res, block_context))
}

fn to_blockifier_tx(
    tx: ExecutableTransactionInput,
    tx_hash: Option<TransactionHash>,
) -> ExecutionResult<BlockifierTransaction> {
    // TODO(yair): Remove the unwrap once the blockifier calculates the tx hash.
    let tx_hash = tx_hash.unwrap_or_default();
    match tx {
        ExecutableTransactionInput::Invoke(invoke_tx) => Ok(BlockifierTransaction::from_api(
            Transaction::Invoke(invoke_tx),
            tx_hash,
            None,
            None,
            None,
        )?),

        ExecutableTransactionInput::Deploy(deploy_acc_tx) => Ok(BlockifierTransaction::from_api(
            Transaction::DeployAccount(deploy_acc_tx),
            tx_hash,
            None,
            None,
            None,
        )?),

        ExecutableTransactionInput::DeclareV0(declare_tx, deprecated_class) => {
            let class_v0 = BlockifierContractClass::V0(deprecated_class.try_into()?);
            Ok(BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V0(declare_tx)),
                tx_hash,
                Some(class_v0),
                None,
                None,
            )?)
        }
        ExecutableTransactionInput::DeclareV1(declare_tx, deprecated_class) => {
            let class_v0 = BlockifierContractClass::V0(deprecated_class.try_into()?);
            Ok(BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V1(declare_tx)),
                tx_hash,
                Some(class_v0),
                None,
                None,
            )?)
        }
        ExecutableTransactionInput::DeclareV2(declare_tx, compiled_class) => {
            let class_v1 = BlockifierContractClass::V1(compiled_class.try_into()?);
            Ok(BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V2(declare_tx)),
                tx_hash,
                Some(class_v1),
                None,
                None,
            )?)
        }
        ExecutableTransactionInput::L1Handler(l1_handler_tx, paid_fee) => {
            Ok(BlockifierTransaction::from_api(
                Transaction::L1Handler(l1_handler_tx),
                tx_hash,
                None,
                Some(paid_fee),
                None,
            )?)
        }
    }
}

/// Simulates a series of transactions and returns the transaction traces and the fee estimations.
#[allow(clippy::too_many_arguments)]
pub fn simulate_transactions(
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    chain_id: &ChainId,
    storage_txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
    execution_config: &BlockExecutionConfig,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<Vec<(TransactionTrace, GasPrice, Fee)>> {
    let (txs_execution_info, block_context) = execute_transactions(
        // TODO(yair): Modify execute_transactions so it doesn't consume the txs / save the tx
        // types before consuming them.
        txs.clone(),
        tx_hashes,
        chain_id,
        storage_txn,
        state_number,
        execution_config,
        charge_fee,
        validate,
    )?;
    Ok(txs
        .iter()
        .zip(txs_execution_info)
        .map(|(tx, exec_info)| calc_trace_and_fee(tx, exec_info, &block_context))
        .collect())
}

fn calc_trace_and_fee(
    tx: &ExecutableTransactionInput,
    execution_info: TransactionExecutionInfo,
    block_context: &BlockContext,
) -> (TransactionTrace, GasPrice, Fee) {
    let gas_price = GasPrice(block_context.gas_price);
    let fee = execution_info.actual_fee;
    let trace = match tx {
        ExecutableTransactionInput::Invoke(_) => TransactionTrace::Invoke(execution_info.into()),
        ExecutableTransactionInput::DeclareV0(_, _) => {
            TransactionTrace::Declare(execution_info.into())
        }
        ExecutableTransactionInput::DeclareV1(_, _) => {
            TransactionTrace::Declare(execution_info.into())
        }
        ExecutableTransactionInput::DeclareV2(_, _) => {
            TransactionTrace::Declare(execution_info.into())
        }
        ExecutableTransactionInput::Deploy(_) => {
            TransactionTrace::DeployAccount(execution_info.into())
        }
        ExecutableTransactionInput::L1Handler(_, _) => {
            TransactionTrace::L1Handler(execution_info.into())
        }
    };
    (trace, gas_price, fee)
}
