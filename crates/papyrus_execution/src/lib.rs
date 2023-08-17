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
use std::iter;
use std::sync::Arc;

use blockifier::abi::constants::STEP_GAS_COST;
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
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector, PatriciaKey};
// TODO: merge multiple EntryPointType structs in SN_API into one.
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointType,
};
use starknet_api::hash::StarkHash;
use starknet_api::state::StateNumber;
use starknet_api::transaction::{
    Calldata,
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    Fee,
    InvokeTransaction,
    Transaction,
    TransactionHash,
};
use starknet_api::{contract_address, patricia_key};
use state_reader::ExecutionStateReader;

/// Result type for execution functions.
pub type ExecutionResult<T> = Result<T, ExecutionError>;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(missing_docs)]
/// Parameters that are needed for execution.
// TODO(yair): Find a way to get them from the Starknet general config.
pub struct ExecutionConfig {
    pub fee_contract_address: ContractAddress,
    pub invoke_tx_max_n_steps: u32,
    pub validate_tx_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub step_gas_cost: u64,
    pub initial_gas_cost: u64,

    // VM_RESOURCE_FEE_COST
    pub n_steps: f64,             // N_STEPS_RESOURCE
    pub pedersen_builtin: f64,    // HASH_BUILTIN_NAME
    pub range_check_builtin: f64, // RANGE_CHECK_BUILTIN_NAME
    pub ecdsa_builtin: f64,       // SIGNATURE_BUILTIN_NAME
    pub bitwise_builtin: f64,     // BITWISE_BUILTIN_NAME
    pub poseidon_builtin: f64,    // POSEIDON_BUILTIN_NAME
    pub output_builtin: f64,      // OUTPUT_BUILTIN_NAME
    pub ec_op_builtin: f64,       // EC_OP_BUILTIN_NAME
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            fee_contract_address: contract_address!(
                "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
            ),
            invoke_tx_max_n_steps: 1_000_000,
            validate_tx_max_n_steps: 1_000_000,
            max_recursion_depth: 50,
            step_gas_cost: STEP_GAS_COST,
            initial_gas_cost: 10_u64.pow(8) * STEP_GAS_COST,
            n_steps: 1_f64,
            pedersen_builtin: 1_f64,
            range_check_builtin: 1_f64,
            ecdsa_builtin: 1_f64,
            bitwise_builtin: 1_f64,
            poseidon_builtin: 1_f64,
            output_builtin: 1_f64,
            ec_op_builtin: 1_f64,
        }
    }
}

impl SerializeConfig for ExecutionConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "fee_contract_address",
                &self.fee_contract_address,
                "The contract address of the ERC-20 fee contract used for paying fees.",
            ),
            ser_param(
                "invoke_tx_max_n_steps",
                &self.invoke_tx_max_n_steps,
                "Max steps for invoke transaction.",
            ),
            ser_param(
                "validate_tx_max_n_steps",
                &self.validate_tx_max_n_steps,
                "Max steps for validating transaction.",
            ),
            ser_param(
                "max_recursion_depth",
                &self.max_recursion_depth,
                "Max recursion depth for transaction.",
            ),
            ser_param("step_gas_cost", &self.step_gas_cost, "Cost of a single step."),
            ser_param(
                "initial_gas_cost",
                &self.initial_gas_cost,
                "An estimation of the initial gas for a transaction to run with (10e8 * \
                 step_gas_cost).",
            ),
            // TODO(yair): fill description.
            ser_param("n_steps", &self.n_steps, "I don't know what this is."),
            ser_param(
                "pedersen_builtin",
                &self.pedersen_builtin,
                "Cost of a single pedersen builtin call.",
            ),
            ser_param(
                "range_check_builtin",
                &self.range_check_builtin,
                "Cost of a single range_check builtin call.",
            ),
            ser_param("ecdsa_builtin", &self.ecdsa_builtin, "Cost of a single ecdsa builtin call."),
            ser_param(
                "bitwise_builtin",
                &self.bitwise_builtin,
                "Cost of a single bitwise builtin call.",
            ),
            ser_param(
                "poseidon_builtin",
                &self.poseidon_builtin,
                "Cost of a single poseidon builtin call.",
            ),
            ser_param(
                "output_builtin",
                &self.output_builtin,
                "Cost of a single output builtin call.",
            ),
            ser_param("ec_op_builtin", &self.ec_op_builtin, "Cost of a single ec_op builtin call."),
        ])
    }
}

impl ExecutionConfig {
    /// Returns the VM resources fee cost as a map from resource name to cost.
    pub fn vm_resources_fee_cost(&self) -> Arc<HashMap<String, f64>> {
        Arc::new(HashMap::from([
            ("n_steps".to_string(), self.n_steps),
            ("pedersen_builtin".to_string(), self.pedersen_builtin),
            ("range_check_builtin".to_string(), self.range_check_builtin),
            ("ecdsa_builtin".to_string(), self.ecdsa_builtin),
            ("bitwise_builtin".to_string(), self.bitwise_builtin),
            ("poseidon_builtin".to_string(), self.poseidon_builtin),
            ("output_builtin".to_string(), self.output_builtin),
            ("ec_op_builtin".to_string(), self.ec_op_builtin),
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
}

/// Executes a StarkNet call and returns the execution result.
pub fn execute_call(
    txn: &StorageTxn<'_, RO>,
    chain_id: &ChainId,
    state_number: StateNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
    execution_config: &ExecutionConfig,
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
        initial_gas: execution_config.initial_gas_cost,
    };
    let mut cached_state = CachedState::from(ExecutionStateReader { txn, state_number });
    let header =
        txn.get_block_header(block_before(state_number))?.expect("Should have block header.");
    let block_context = create_block_context(
        chain_id.clone(),
        state_number.0,
        header.timestamp,
        header.gas_price,
        &header.sequencer,
        execution_config,
    );
    let mut context = EntryPointExecutionContext::new(
        block_context,
        AccountTransactionContext::default(),
        execution_config.invoke_tx_max_n_steps as usize,
    );

    let res = call_entry_point.execute(
        &mut cached_state,
        &mut ExecutionResources::default(),
        &mut context,
    )?;

    Ok(res.execution)
}

// TODO(yair): Move to StarknetAPI.
// If the state was created using StateNumber::right_after_block(block_number), then the function
// will return the block number.
fn block_before(state_number: StateNumber) -> BlockNumber {
    state_number.block_after().prev().unwrap_or_default()
}

fn verify_node_synced(txn: &StorageTxn<'_, RO>, state_number: StateNumber) -> ExecutionResult<()> {
    let compiled_class_marker = txn.get_compiled_class_marker()?;
    let synced_up_to = StateNumber::right_before_block(compiled_class_marker);
    if state_number > synced_up_to {
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
    execution_config: &ExecutionConfig,
) -> BlockContext {
    BlockContext {
        chain_id,
        block_number,
        block_timestamp,
        sequencer_address: *sequencer_address,
        fee_token_address: execution_config.fee_contract_address,
        vm_resource_fee_cost: execution_config.vm_resources_fee_cost(),
        invoke_tx_max_n_steps: execution_config.invoke_tx_max_n_steps,
        validate_max_n_steps: execution_config.validate_tx_max_n_steps,
        max_recursion_depth: execution_config.max_recursion_depth,
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
}

/// Returns the fee estimation for a series of transactions.
pub fn estimate_fee(
    txs: Vec<ExecutableTransactionInput>,
    chain_id: &ChainId,
    storage_txn: &StorageTxn<'_, RO>,
    state_number: StateNumber,
    execution_config: &ExecutionConfig,
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
    execution_config: &ExecutionConfig,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<(Vec<TransactionExecutionInfo>, BlockContext)> {
    verify_node_synced(storage_txn, state_number)?;
    let header = storage_txn
        .get_block_header(block_before(state_number))?
        .expect("Should have block header.");

    let mut cached_state =
        CachedState::from(ExecutionStateReader { txn: storage_txn, state_number });
    let block_context = create_block_context(
        chain_id.clone(),
        block_before(state_number),
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
    execution_config: &ExecutionConfig,
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
        .zip(txs_execution_info.into_iter())
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
    };
    (trace, gas_price, fee)
}
