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

use std::collections::HashMap;
use std::iter;
use std::sync::Arc;

use blockifier::abi::constants::{INITIAL_GAS_COST, N_STEPS_RESOURCE};
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
use cairo_vm::vm::runners::builtin_runner::{
    BITWISE_BUILTIN_NAME,
    EC_OP_BUILTIN_NAME,
    HASH_BUILTIN_NAME,
    OUTPUT_BUILTIN_NAME,
    POSEIDON_BUILTIN_NAME,
    RANGE_CHECK_BUILTIN_NAME,
    SIGNATURE_BUILTIN_NAME,
};
use objects::TransactionTrace;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageTxn};
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
    Transaction,
    TransactionHash,
};
use state_reader::ExecutionStateReader;

/// Result type for execution functions.
pub type ExecutionResult<T> = Result<T, ExecutionError>;
use lazy_static::lazy_static;

// TODO(yair): These constants should be taken from the Starknet global config.
const INVOKE_TX_MAX_N_STEPS: u32 = 1_000_000;
const VALIDATE_TX_MAX_N_STEPS: u32 = 1_000_000;
const MAX_RECURSION_DEPTH: usize = 50;

lazy_static! {
    // TODO(yair): get real values.
    static ref VM_RESOURCE_FEE_COST: Arc<HashMap<String, f64>> =  Arc::new(HashMap::from([
        (N_STEPS_RESOURCE.to_string(), 1_f64),
        (HASH_BUILTIN_NAME.to_string(), 1_f64),
        (RANGE_CHECK_BUILTIN_NAME.to_string(), 1_f64),
        (SIGNATURE_BUILTIN_NAME.to_string(), 1_f64),
        (BITWISE_BUILTIN_NAME.to_string(), 1_f64),
        (POSEIDON_BUILTIN_NAME.to_string(), 1_f64),
        (OUTPUT_BUILTIN_NAME.to_string(), 1_f64),
        (EC_OP_BUILTIN_NAME.to_string(), 1_f64),
    ]));
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
    #[error("Must provide fee token contract address when passing charge_fee or validate.")]
    MissingFeeTokenContractAddress,
    #[error("Executing transactions without passing transaction hashes is not supported yet.")]
    MissingTransactionHashes
}

/// Executes a StarkNet call and returns the execution result.
pub fn execute_call(
    txn: &StorageTxn<'_, RO>,
    chain_id: &ChainId,
    state_number: StateNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
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
        // todo(yair): Check if this is the correct value.
        initial_gas: INITIAL_GAS_COST,
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
        None,
    );
    let mut context = EntryPointExecutionContext::new(
        block_context,
        AccountTransactionContext::default(),
        INVOKE_TX_MAX_N_STEPS as usize,
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
    fee_contract_address: Option<ContractAddress>,
) -> BlockContext {
    BlockContext {
        chain_id,
        block_number,
        block_timestamp,
        sequencer_address: *sequencer_address,
        fee_token_address: fee_contract_address.unwrap_or_default(),
        vm_resource_fee_cost: VM_RESOURCE_FEE_COST.clone(),
        invoke_tx_max_n_steps: INVOKE_TX_MAX_N_STEPS,
        validate_max_n_steps: VALIDATE_TX_MAX_N_STEPS,
        max_recursion_depth: MAX_RECURSION_DEPTH,
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
) -> ExecutionResult<Vec<(GasPrice, Fee)>> {
    let (txs_execution_info, block_context) =
        execute_transactions(txs, None, chain_id, storage_txn, state_number, None, false, false)?;
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
    fee_contract_address: Option<ContractAddress>,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<(Vec<TransactionExecutionInfo>, BlockContext)> {
    if tx_hashes.is_none() {
        return Err(ExecutionError::MissingTransactionHashes);
    }
    if fee_contract_address.is_none() && (charge_fee || validate) {
        return Err(ExecutionError::MissingFeeTokenContractAddress);
    }
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
        fee_contract_address,
    );

    let tx_hashes_iter: Box<dyn Iterator<Item = TransactionHash>> = match tx_hashes {
        Some(hashes) => Box::new(hashes.into_iter()),
        None => Box::new(iter::repeat(TransactionHash::default())),
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
    tx_hash: TransactionHash,
) -> ExecutionResult<BlockifierTransaction> {
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
    // Can be None if we don't want to charge fees or validate.
    fee_contract_address: Option<ContractAddress>,
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
        fee_contract_address,
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
