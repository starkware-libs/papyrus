#![warn(missing_docs)]
//! Functionality for executing Starknet transactions and contract entry points.
//!
//! In this module, we use the term "state_number" to refer to the state of the storage at the
//! execution, and "block_context_block_number" to refer to the block in which the transactions
//! should run. For example, if you want to simulate transactions at the beginning of block 10, you
//! should use state_number = 10 and block_context_block_number = 10. If you want to simulate
//! transactions at the end of block 10, you should use state_number = 11 and
//! block_context_block_number = 10.
//! See documentation of [StateNumber] for more details.
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
use std::sync::Arc;

use blockifier::block_context::{BlockContext, FeeTokenAddresses, GasPrices};
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::execution::entry_point::{
    CallEntryPoint,
    CallType as BlockifierCallType,
    EntryPointExecutionContext,
    ExecutionResources,
};
use blockifier::execution::errors::{EntryPointExecutionError, PreExecutionError};
use blockifier::state::cached_state::CachedState;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::transaction::objects::{
    AccountTransactionContext,
    DeprecatedAccountTransactionContext,
    TransactionExecutionInfo,
};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::transaction::transactions::ExecutableTransaction;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use cairo_vm::types::errors::program_errors::ProgramError;
use execution_utils::{get_trace_constructor, induced_state_diff};
use objects::TransactionTrace;
use papyrus_common::transaction_hash::get_transaction_hash;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::RO;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageTxn};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector};
// TODO: merge multiple EntryPointType structs in SN_API into one.
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointType,
};
use starknet_api::state::{StateNumber, ThinStateDiff};
use starknet_api::transaction::{
    Calldata,
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeclareTransactionV3,
    DeployAccountTransaction,
    Fee,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction,
    TransactionHash,
};
use starknet_api::StarknetApiError;
use state_reader::ExecutionStateReader;
use tracing::trace;

use crate::objects::PendingData;

/// Result type for execution functions.
pub type ExecutionResult<T> = Result<T, ExecutionError>;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
/// Parameters that are needed for execution.
// TODO(yair): Find a way to get them from the Starknet general config.
pub struct BlockExecutionConfig {
    /// The adress to receive fees
    pub fee_contract_address: ContractAddress,
    /// The maximum number of steps for an invoke transaction
    pub invoke_tx_max_n_steps: u32,
    /// The maximum number of steps for a validate transaction
    pub validate_tx_max_n_steps: u32,
    /// The maximum recursion depth for a transaction
    pub max_recursion_depth: usize,
    /// The cost of a single step
    pub step_gas_cost: u64,
    /// Parameter used to calculate the fee for a transaction
    pub vm_resource_fee_cost: Arc<HashMap<String, f64>>,
    /// The initial gas cost for a transaction
    pub initial_gas_cost: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
/// Holds a mapping from the block number, to the corresponding execution configuration.
pub struct ExecutionConfigByBlock {
    /// A mapping from the block number to the execution configuration corresponding to the version
    /// that was updated in this block.
    pub execution_config_segments: BTreeMap<BlockNumber, BlockExecutionConfig>,
}

impl ExecutionConfigByBlock {
    /// Returns the execution config for a given block number.
    pub fn get_execution_config_for_block(
        &self,
        block_number: BlockNumber,
    ) -> ExecutionResult<&BlockExecutionConfig> {
        let segments = &self.execution_config_segments;
        if segments.is_empty() || segments.keys().min() != Some(&BlockNumber(0)) {
            return Err(ExecutionError::ConfigContentError);
        }

        // TODO(yael): use the upper_bound feature once stable
        // Ok(segments.upper_bound(std::ops::Bound::Included(&block_number)).value().unwrap().
        // clone())

        for (segment_block_number, segment) in segments.iter().rev() {
            if block_number >= *segment_block_number {
                return Ok(segment);
            }
        }
        Err(ExecutionError::ConfigContentError)
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
    #[error("Failed to calculate transaction hash.")]
    TransactionHashCalculationFailed(StarknetApiError),
    #[error(transparent)]
    PreExecutionError(#[from] PreExecutionError),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error("Charging fee is not supported yet in execution.")]
    ChargeFeeNotSupported,
    #[error("Execution config file does not contain a configuration for all blocks")]
    ConfigContentError,
    #[error(transparent)]
    ConfigFileError(#[from] std::io::Error),
    #[error(transparent)]
    ConfigSerdeError(#[from] serde_json::Error),
    #[error("Missing class hash in call info")]
    MissingClassHash,
}

/// Executes a StarkNet call and returns the execution result.
#[allow(clippy::too_many_arguments)]
pub fn execute_call(
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    chain_id: &ChainId,
    state_number: StateNumber,
    block_context_number: BlockNumber,
    contract_address: &ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
    execution_config: &BlockExecutionConfig,
) -> ExecutionResult<CallExecution> {
    verify_node_synced(&storage_reader.begin_ro_txn()?, block_context_number, state_number)?;
    verify_contract_exists(
        *contract_address,
        &storage_reader,
        state_number,
        maybe_pending_data.as_ref(),
    )?;

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

    let block_context = create_block_context(
        block_context_number,
        chain_id.clone(),
        &storage_reader,
        maybe_pending_data.as_ref(),
        execution_config,
    )?;

    let mut cached_state = CachedState::from(ExecutionStateReader {
        storage_reader,
        state_number,
        maybe_pending_data,
    });
    let mut context = EntryPointExecutionContext::new_invoke(
        &block_context,
        // TODO(yair): fix when supporting v3 transactions
        &AccountTransactionContext::Deprecated(DeprecatedAccountTransactionContext::default()),
        true, // limit_steps_by_resources
    )?;

    let res = call_entry_point.execute(
        &mut cached_state,
        &mut ExecutionResources::default(),
        &mut context,
    )?;

    Ok(res.execution)
}

fn verify_node_synced(
    txn: &StorageTxn<'_, RO>,
    block_context_number: BlockNumber,
    state_number: StateNumber,
) -> ExecutionResult<()> {
    let compiled_class_marker = txn.get_compiled_class_marker()?;
    if block_context_number >= compiled_class_marker || state_number.is_after(compiled_class_marker)
    {
        return Err(ExecutionError::NotSynced {
            state_number: StateNumber::right_after_block(block_context_number),
            compiled_class_marker,
        });
    }
    Ok(())
}

fn verify_contract_exists(
    contract_address: ContractAddress,
    storage_reader: &StorageReader,
    state_number: StateNumber,
    maybe_pending_data: Option<&PendingData>,
) -> ExecutionResult<()> {
    execution_utils::get_class_hash_at(
        storage_reader,
        state_number,
        maybe_pending_data.map(|pending_state_diff| &pending_state_diff.deployed_contracts),
        contract_address,
    )?
    .ok_or(ExecutionError::ContractNotFound { contract_address, state_number })?;
    Ok(())
}

fn create_block_context(
    block_context_number: BlockNumber,
    chain_id: ChainId,
    storage_reader: &StorageReader,
    maybe_pending_data: Option<&PendingData>,
    execution_config: &BlockExecutionConfig,
) -> ExecutionResult<BlockContext> {
    let (block_number, block_timestamp, gas_prices, sequencer_address) = match maybe_pending_data {
        Some(pending_data) => (
            block_context_number.next(),
            pending_data.timestamp,
            GasPrices {
                eth_l1_gas_price: pending_data.eth_l1_gas_price.0,
                strk_l1_gas_price: pending_data.strk_l1_gas_price.0,
            },
            pending_data.sequencer,
        ),
        None => {
            let header = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_context_number)?
                .expect("Should have block header.");
            (
                header.block_number,
                header.timestamp,
                GasPrices {
                    eth_l1_gas_price: header.eth_l1_gas_price.0,
                    strk_l1_gas_price: header.strk_l1_gas_price.0,
                },
                header.sequencer,
            )
        }
    };

    Ok(BlockContext {
        chain_id,
        block_number,
        block_timestamp,
        sequencer_address,
        // TODO(barak, 01/10/2023): Change strk_fee_token_address once it exists.
        fee_token_addresses: FeeTokenAddresses {
            strk_fee_token_address: execution_config.fee_contract_address,
            eth_fee_token_address: execution_config.fee_contract_address,
        },
        vm_resource_fee_cost: Arc::clone(&execution_config.vm_resource_fee_cost),
        invoke_tx_max_n_steps: execution_config.invoke_tx_max_n_steps,
        validate_max_n_steps: execution_config.validate_tx_max_n_steps,
        max_recursion_depth: execution_config.max_recursion_depth,
        // TODO(barak, 01/10/2023): Change strk_l1_gas_price once it exists.
        gas_prices,
    })
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
    DeclareV3(DeclareTransactionV3, CasmContractClass),
    DeployAccount(DeployAccountTransaction),
    L1Handler(L1HandlerTransaction, Fee),
}

impl ExecutableTransactionInput {
    fn calc_tx_hash(self, chain_id: &ChainId) -> ExecutionResult<(Self, TransactionHash)> {
        match self.apply_on_transaction(|tx| get_transaction_hash(tx, chain_id)) {
            (original_tx, Ok(tx_hash)) => Ok((original_tx, tx_hash)),
            (_, Err(err)) => Err(ExecutionError::TransactionHashCalculationFailed(err)),
        }
    }

    /// Applies a non consuming function on the transaction as if it was of type [Transaction] of
    /// StarknetAPI and returns the result without cloning the original transaction.
    // TODO(yair): Refactor this.
    fn apply_on_transaction<F, T>(self, func: F) -> (Self, T)
    where
        F: Fn(&Transaction) -> T,
    {
        match self {
            ExecutableTransactionInput::Invoke(tx) => {
                let as_transaction = Transaction::Invoke(tx);
                let res = func(&as_transaction);
                let Transaction::Invoke(tx) = as_transaction else {
                    unreachable!("Should be invoke transaction.")
                };
                (Self::Invoke(tx), res)
            }
            ExecutableTransactionInput::DeclareV0(tx, class) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V0(tx));
                let res = func(&as_transaction);
                let Transaction::Declare(DeclareTransaction::V0(tx)) = as_transaction else {
                    unreachable!("Should be declare v0 transaction.")
                };
                (Self::DeclareV0(tx, class), res)
            }
            ExecutableTransactionInput::DeclareV1(tx, class) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V1(tx));
                let res = func(&as_transaction);
                let Transaction::Declare(DeclareTransaction::V1(tx)) = as_transaction else {
                    unreachable!("Should be declare v1 transaction.")
                };
                (Self::DeclareV1(tx, class), res)
            }
            ExecutableTransactionInput::DeclareV2(tx, class) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V2(tx));
                let res = func(&as_transaction);
                let Transaction::Declare(DeclareTransaction::V2(tx)) = as_transaction else {
                    unreachable!("Should be declare v2 transaction.")
                };
                (Self::DeclareV2(tx, class), res)
            }
            ExecutableTransactionInput::DeclareV3(tx, class) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V3(tx));
                let res = func(&as_transaction);
                let Transaction::Declare(DeclareTransaction::V3(tx)) = as_transaction else {
                    unreachable!("Should be declare v3 transaction.")
                };
                (Self::DeclareV3(tx, class), res)
            }
            ExecutableTransactionInput::DeployAccount(tx) => {
                let as_transaction = Transaction::DeployAccount(tx);
                let res = func(&as_transaction);
                let Transaction::DeployAccount(tx) = as_transaction else {
                    unreachable!("Should be deploy account transaction.")
                };
                (Self::DeployAccount(tx), res)
            }
            ExecutableTransactionInput::L1Handler(tx, fee) => {
                let as_transaction = Transaction::L1Handler(tx);
                let res = func(&as_transaction);
                let Transaction::L1Handler(tx) = as_transaction else {
                    unreachable!("Should be L1 handler transaction.")
                };
                (Self::L1Handler(tx, fee), res)
            }
        }
    }
}

/// Calculates the transaction hashes for a series of transactions without cloning the transactions.
fn calc_tx_hashes(
    txs: Vec<ExecutableTransactionInput>,
    chain_id: &ChainId,
) -> ExecutionResult<(Vec<ExecutableTransactionInput>, Vec<TransactionHash>)> {
    Ok(txs
        .into_iter()
        .map(|tx| tx.calc_tx_hash(chain_id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .unzip())
}

/// Returns the fee estimation for a series of transactions.
#[allow(clippy::too_many_arguments)]
pub fn estimate_fee(
    txs: Vec<ExecutableTransactionInput>,
    chain_id: &ChainId,
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    state_number: StateNumber,
    block_context_block_number: BlockNumber,
    execution_config: &BlockExecutionConfig,
) -> ExecutionResult<Vec<(GasPrice, Fee)>> {
    let (txs_execution_info, block_context) = execute_transactions(
        txs,
        None,
        chain_id,
        storage_reader,
        maybe_pending_data,
        state_number,
        block_context_block_number,
        execution_config,
        false,
        false,
    )?;
    Ok(txs_execution_info
        .into_iter()
        .map(|(tx_execution_info, _)| {
            (GasPrice(block_context.gas_prices.eth_l1_gas_price), tx_execution_info.actual_fee)
        })
        .collect())
}

// Executes a series of transactions and returns the execution results.
// TODO(yair): Return structs instead of tuples.
#[allow(clippy::too_many_arguments)]
fn execute_transactions(
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    chain_id: &ChainId,
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    state_number: StateNumber,
    block_context_block_number: BlockNumber,
    execution_config: &BlockExecutionConfig,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<(Vec<(TransactionExecutionInfo, ThinStateDiff)>, BlockContext)> {
    {
        let storage_txn = storage_reader.begin_ro_txn()?;
        verify_node_synced(&storage_txn, block_context_block_number, state_number)?;
    }

    let block_context = create_block_context(
        block_context_block_number,
        chain_id.clone(),
        &storage_reader,
        maybe_pending_data.as_ref(),
        execution_config,
    )?;

    // The starknet state will be from right before the block in which the transactions should run.
    let mut cached_state = CachedState::from(ExecutionStateReader {
        storage_reader,
        state_number,
        maybe_pending_data,
    });

    let (txs, tx_hashes) = match tx_hashes {
        Some(tx_hashes) => (txs, tx_hashes),
        None => {
            let tx_hashes = calc_tx_hashes(txs, chain_id)?;
            trace!("Calculated tx hashes: {:?}", tx_hashes);
            tx_hashes
        }
    };

    let mut res = vec![];
    for (tx, tx_hash) in txs.into_iter().zip(tx_hashes.into_iter()) {
        let mut transactional_state = CachedState::create_transactional(&mut cached_state);
        let deprecated_declared_class_hash = match &tx {
            ExecutableTransactionInput::DeclareV0(DeclareTransactionV0V1 { class_hash, .. }, _) => {
                Some(*class_hash)
            }
            ExecutableTransactionInput::DeclareV1(DeclareTransactionV0V1 { class_hash, .. }, _) => {
                Some(*class_hash)
            }
            _ => None,
        };
        let blockifier_tx = to_blockifier_tx(tx, tx_hash)?;
        let tx_execution_info = blockifier_tx.execute(
            &mut transactional_state,
            &block_context,
            charge_fee,
            validate,
        )?;
        let state_diff =
            induced_state_diff(&mut transactional_state, deprecated_declared_class_hash)?;
        transactional_state.commit();
        res.push((tx_execution_info, state_diff));
    }

    Ok((res, block_context))
}

fn to_blockifier_tx(
    tx: ExecutableTransactionInput,
    tx_hash: TransactionHash,
) -> ExecutionResult<BlockifierTransaction> {
    // TODO(yair): support only_query version bit (enable in the RPC v0.6 and use the correct
    // value).
    const ONLY_QUERY: bool = false;
    match tx {
        ExecutableTransactionInput::Invoke(invoke_tx) => Ok(BlockifierTransaction::from_api(
            Transaction::Invoke(invoke_tx),
            tx_hash,
            None,
            None,
            None,
            ONLY_QUERY,
        )?),

        ExecutableTransactionInput::DeployAccount(deploy_acc_tx) => {
            Ok(BlockifierTransaction::from_api(
                Transaction::DeployAccount(deploy_acc_tx),
                tx_hash,
                None,
                None,
                None,
                ONLY_QUERY,
            )?)
        }

        ExecutableTransactionInput::DeclareV0(declare_tx, deprecated_class) => {
            let class_v0 = BlockifierContractClass::V0(deprecated_class.try_into()?);
            Ok(BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V0(declare_tx)),
                tx_hash,
                Some(class_v0),
                None,
                None,
                ONLY_QUERY,
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
                ONLY_QUERY,
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
                ONLY_QUERY,
            )?)
        }
        ExecutableTransactionInput::DeclareV3(declare_tx, compiled_class) => {
            let class_v1 = BlockifierContractClass::V1(compiled_class.try_into()?);
            Ok(BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V3(declare_tx)),
                tx_hash,
                Some(class_v1),
                None,
                None,
                ONLY_QUERY,
            )?)
        }
        ExecutableTransactionInput::L1Handler(l1_handler_tx, paid_fee) => {
            Ok(BlockifierTransaction::from_api(
                Transaction::L1Handler(l1_handler_tx),
                tx_hash,
                None,
                Some(paid_fee),
                None,
                ONLY_QUERY,
            )?)
        }
    }
}

/// Simulates a series of transactions and returns the transaction traces and the fee estimations.
// TODO(yair): Return structs instead of tuples.
#[allow(clippy::too_many_arguments)]
pub fn simulate_transactions(
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    chain_id: &ChainId,
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    state_number: StateNumber,
    block_context_block_number: BlockNumber,
    execution_config: &BlockExecutionConfig,
    charge_fee: bool,
    validate: bool,
) -> ExecutionResult<Vec<(TransactionTrace, ThinStateDiff, GasPrice, Fee)>> {
    let trace_constructors = txs.iter().map(get_trace_constructor).collect::<Vec<_>>();
    let (execution_results, block_context) = execute_transactions(
        txs,
        tx_hashes,
        chain_id,
        storage_reader,
        maybe_pending_data,
        state_number,
        block_context_block_number,
        execution_config,
        charge_fee,
        validate,
    )?;
    let gas_price = GasPrice(block_context.gas_prices.eth_l1_gas_price);
    execution_results
        .into_iter()
        .zip(trace_constructors)
        .map(|((execution_info, state_diff), trace_constructor)| {
            let fee = execution_info.actual_fee;
            match trace_constructor(execution_info) {
                Ok(trace) => Ok((trace, state_diff, gas_price, fee)),
                Err(e) => Err(e),
            }
        })
        .collect()
}
