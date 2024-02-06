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

use blockifier::block_context::{BlockContext, BlockInfo, ChainInfo, FeeTokenAddresses, GasPrices};
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::contract_class::ContractClass as BlockifierContractClass;
use blockifier::execution::entry_point::{
    CallEntryPoint,
    CallType as BlockifierCallType,
    EntryPointExecutionContext,
    ExecutionResources,
};
use blockifier::state::cached_state::{CachedState, GlobalContractCache};
use blockifier::state::state_api::State;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use blockifier::transaction::objects::{
    AccountTransactionContext,
    DeprecatedAccountTransactionContext,
    TransactionExecutionInfo,
};
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier::transaction::transactions::ExecutableTransaction;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use execution_utils::{get_trace_constructor, induced_state_diff};
use objects::{PriceUnit, TransactionSimulationOutput};
use papyrus_common::transaction_hash::get_transaction_hash;
use papyrus_common::TransactionOptions;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ClassHash, ContractAddress, EntryPointSelector};
// TODO: merge multiple EntryPointType structs in SN_API into one.
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointType,
};
use starknet_api::state::{StateNumber, StorageKey, ThinStateDiff};
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
    TransactionVersion,
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
/// The error type for the execution module.
#[derive(thiserror::Error, Debug)]
pub enum ExecutionError {
    #[error("Execution config file does not contain a configuration for all blocks")]
    ConfigContentError,
    #[error(transparent)]
    ConfigFileError(#[from] std::io::Error),
    #[error(transparent)]
    ConfigSerdeError(#[from] serde_json::Error),
    #[error(transparent)]
    ContractError(#[from] BlockifierError),
    #[error(
        "The contract at address {contract_address:?} is not found at state number \
         {state_number:?}."
    )]
    ContractNotFound { contract_address: ContractAddress, state_number: StateNumber },
    #[error("Missing class hash in call info")]
    MissingClassHash,
    #[error("Missing compiled class with hash {class_hash} (The CASM table isn't synced)")]
    MissingCompiledClass { class_hash: ClassHash },
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(
        "Execution failed at transaction {transaction_index:?} with error: {execution_error:?}"
    )]
    TransactionExecutionError { transaction_index: usize, execution_error: String },
    #[error("Failed to calculate transaction hash.")]
    TransactionHashCalculationFailed(StarknetApiError),
    #[error("Unknown builtin name: {builtin_name}")]
    UnknownBuiltin { builtin_name: String },
}

/// Whether the only-query bit of the transaction version is on.
pub type OnlyQuery = bool;

/// Gathers all the possible errors that can be returned from the blockifier.
type BlockifierError = anyhow::Error;

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

    let mut cached_state = CachedState::new(
        ExecutionStateReader {
            storage_reader,
            state_number,
            maybe_pending_data,
            missing_compiled_class: None,
        },
        GlobalContractCache::new(100),
    );
    let mut context = EntryPointExecutionContext::new_invoke(
        &block_context,
        // TODO(yair): fix when supporting v3 transactions
        &AccountTransactionContext::Deprecated(DeprecatedAccountTransactionContext::default()),
        true, // limit_steps_by_resources
    )
    .map_err(|err| ExecutionError::ContractError(err.into()))?;

    let res = call_entry_point
        .execute(&mut cached_state, &mut ExecutionResources::default(), &mut context)
        .map_err(|error| {
            if let Some(class_hash) = cached_state.state.missing_compiled_class {
                ExecutionError::MissingCompiledClass { class_hash }
            } else {
                ExecutionError::ContractError(error.into())
            }
        })?;

    Ok(res.execution)
}

fn verify_contract_exists(
    contract_address: ContractAddress,
    storage_reader: &StorageReader,
    state_number: StateNumber,
    maybe_pending_data: Option<&PendingData>,
) -> ExecutionResult<()> {
    execution_utils::get_class_hash_at(
        &storage_reader.begin_ro_txn()?,
        state_number,
        maybe_pending_data.map(|pending_state_diff| {
            (&pending_state_diff.deployed_contracts, &pending_state_diff.replaced_classes)
        }),
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
                eth_l1_data_gas_price: 0,
                strk_l1_data_gas_price: 0,
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
                    eth_l1_data_gas_price: 0,
                    strk_l1_data_gas_price: 0,
                },
                header.sequencer,
            )
        }
    };

    Ok(BlockContext {
        // TODO(barak, 01/10/2023): Change strk_l1_gas_price once it exists.
        block_info: BlockInfo {
            block_number,
            block_timestamp,
            // Fee-related.
            sequencer_address,
            vm_resource_fee_cost: Arc::clone(&execution_config.vm_resource_fee_cost),
            gas_prices,
            use_kzg_da: false,
            // Limits.
            invoke_tx_max_n_steps: execution_config.invoke_tx_max_n_steps,
            validate_max_n_steps: execution_config.validate_tx_max_n_steps,
            max_recursion_depth: execution_config.max_recursion_depth,
        },
        chain_info: ChainInfo {
            chain_id,
            fee_token_addresses: FeeTokenAddresses {
                strk_fee_token_address: execution_config.fee_contract_address,
                eth_fee_token_address: execution_config.fee_contract_address,
            },
        },
    })
}

/// The transaction input to be executed.
// TODO(yair): This should use broadcasted transactions instead of regular transactions, but the
// blockifier expects regular transactions. Consider changing the blockifier to use broadcasted txs.
#[allow(missing_docs)]
#[derive(Clone, Debug)]
pub enum ExecutableTransactionInput {
    Invoke(InvokeTransaction, OnlyQuery),
    // todo(yair): Do we need to support V0?
    DeclareV0(DeclareTransactionV0V1, DeprecatedContractClass, OnlyQuery),
    DeclareV1(DeclareTransactionV0V1, DeprecatedContractClass, OnlyQuery),
    DeclareV2(DeclareTransactionV2, CasmContractClass, OnlyQuery),
    DeclareV3(DeclareTransactionV3, CasmContractClass, OnlyQuery),
    DeployAccount(DeployAccountTransaction, OnlyQuery),
    L1Handler(L1HandlerTransaction, Fee, OnlyQuery),
}

impl ExecutableTransactionInput {
    fn calc_tx_hash(self, chain_id: &ChainId) -> ExecutionResult<(Self, TransactionHash)> {
        match self.apply_on_transaction(|tx, only_query| {
            get_transaction_hash(tx, chain_id, &TransactionOptions { only_query })
        }) {
            (original_tx, Ok(tx_hash)) => Ok((original_tx, tx_hash)),
            (_, Err(err)) => Err(ExecutionError::TransactionHashCalculationFailed(err)),
        }
    }

    /// Applies a non consuming function on the transaction as if it was of type [Transaction] of
    /// StarknetAPI and returns the result without cloning the original transaction.
    // TODO(yair): Refactor this.
    fn apply_on_transaction<F, T>(self, func: F) -> (Self, T)
    where
        F: Fn(&Transaction, OnlyQuery) -> T,
    {
        match self {
            ExecutableTransactionInput::Invoke(tx, only_query) => {
                let as_transaction = Transaction::Invoke(tx);
                let res = func(&as_transaction, only_query);
                let Transaction::Invoke(tx) = as_transaction else {
                    unreachable!("Should be invoke transaction.")
                };
                (Self::Invoke(tx, only_query), res)
            }
            ExecutableTransactionInput::DeclareV0(tx, class, only_query) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V0(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V0(tx)) = as_transaction else {
                    unreachable!("Should be declare v0 transaction.")
                };
                (Self::DeclareV0(tx, class, only_query), res)
            }
            ExecutableTransactionInput::DeclareV1(tx, class, only_query) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V1(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V1(tx)) = as_transaction else {
                    unreachable!("Should be declare v1 transaction.")
                };
                (Self::DeclareV1(tx, class, only_query), res)
            }
            ExecutableTransactionInput::DeclareV2(tx, class, only_query) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V2(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V2(tx)) = as_transaction else {
                    unreachable!("Should be declare v2 transaction.")
                };
                (Self::DeclareV2(tx, class, only_query), res)
            }
            ExecutableTransactionInput::DeclareV3(tx, class, only_query) => {
                let as_transaction = Transaction::Declare(DeclareTransaction::V3(tx));
                let res = func(&as_transaction, only_query);
                let Transaction::Declare(DeclareTransaction::V3(tx)) = as_transaction else {
                    unreachable!("Should be declare v3 transaction.")
                };
                (Self::DeclareV3(tx, class, only_query), res)
            }
            ExecutableTransactionInput::DeployAccount(tx, only_query) => {
                let as_transaction = Transaction::DeployAccount(tx);
                let res = func(&as_transaction, only_query);
                let Transaction::DeployAccount(tx) = as_transaction else {
                    unreachable!("Should be deploy account transaction.")
                };
                (Self::DeployAccount(tx, only_query), res)
            }
            ExecutableTransactionInput::L1Handler(tx, fee, only_query) => {
                let as_transaction = Transaction::L1Handler(tx);
                let res = func(&as_transaction, only_query);
                let Transaction::L1Handler(tx) = as_transaction else {
                    unreachable!("Should be L1 handler transaction.")
                };
                (Self::L1Handler(tx, fee, only_query), res)
            }
        }
    }

    /// Returns the transaction version.
    pub fn transaction_version(&self) -> TransactionVersion {
        match self {
            ExecutableTransactionInput::Invoke(tx, ..) => tx.version(),
            ExecutableTransactionInput::DeclareV0(..) => TransactionVersion::ZERO,
            ExecutableTransactionInput::DeclareV1(..) => TransactionVersion::ONE,
            ExecutableTransactionInput::DeclareV2(..) => TransactionVersion::TWO,
            ExecutableTransactionInput::DeclareV3(..) => TransactionVersion::THREE,
            ExecutableTransactionInput::DeployAccount(tx, ..) => tx.version(),
            ExecutableTransactionInput::L1Handler(tx, ..) => tx.version,
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

/// Output for fee estimation when a transaction reverted.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RevertedTransaction {
    /// The index of the reverted transaction.
    pub index: usize,
    /// The revert reason.
    pub revert_reason: String,
}

/// Valid output for fee estimation for a series of transactions can be either a list of fees or the
/// index and revert reason of the first reverted transaction.
pub type FeeEstimationResult = Result<Vec<(GasPrice, Fee, PriceUnit)>, RevertedTransaction>;

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
    validate: bool,
) -> ExecutionResult<FeeEstimationResult> {
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
        validate,
    )?;
    Ok(txs_execution_info
        .into_iter()
        .enumerate()
        .map(|(index, tx_execution_output)| {
            // If the transaction reverted, fail the entire estimation.
            if let Some(revert_reason) = tx_execution_output.execution_info.revert_error {
                Err(RevertedTransaction { index, revert_reason })
            } else {
                let gas_price = match tx_execution_output.price_unit {
                    PriceUnit::Wei => {
                        GasPrice(block_context.block_info.gas_prices.eth_l1_gas_price)
                    }
                    PriceUnit::Fri => {
                        GasPrice(block_context.block_info.gas_prices.strk_l1_gas_price)
                    }
                };
                Ok((
                    gas_price,
                    tx_execution_output.execution_info.actual_fee,
                    tx_execution_output.price_unit,
                ))
            }
        })
        .collect())
}

struct TransactionExecutionOutput {
    execution_info: TransactionExecutionInfo,
    induced_state_diff: ThinStateDiff,
    price_unit: PriceUnit,
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
) -> ExecutionResult<(Vec<TransactionExecutionOutput>, BlockContext)> {
    let block_context = create_block_context(
        block_context_block_number,
        chain_id.clone(),
        &storage_reader,
        maybe_pending_data.as_ref(),
        execution_config,
    )?;

    // The starknet state will be from right before the block in which the transactions should run.
    let mut cached_state = CachedState::new(
        ExecutionStateReader {
            storage_reader,
            state_number,
            maybe_pending_data,
            missing_compiled_class: None,
        },
        GlobalContractCache::new(100),
    );

    // TODO(yair): this is a temporary bug fix, delete once the blockifier is fixed and add a test.
    set_block_hash_contract(state_number, &mut cached_state)?;

    let (txs, tx_hashes) = match tx_hashes {
        Some(tx_hashes) => (txs, tx_hashes),
        None => {
            let tx_hashes = calc_tx_hashes(txs, chain_id)?;
            trace!("Calculated tx hashes: {:?}", tx_hashes);
            tx_hashes
        }
    };

    let mut res = vec![];
    for (transaction_index, (tx, tx_hash)) in txs.into_iter().zip(tx_hashes.into_iter()).enumerate()
    {
        let price_unit = if tx.transaction_version() == TransactionVersion::ZERO
            || tx.transaction_version() == TransactionVersion::ONE
            || tx.transaction_version() == TransactionVersion::TWO
        {
            PriceUnit::Wei
        }
        // From V3 all transactions are priced in Fri.
        else {
            PriceUnit::Fri
        };
        let mut transactional_state = CachedState::create_transactional(&mut cached_state);
        let deprecated_declared_class_hash = match &tx {
            ExecutableTransactionInput::DeclareV0(
                DeclareTransactionV0V1 { class_hash, .. },
                _,
                _,
            ) => Some(*class_hash),
            ExecutableTransactionInput::DeclareV1(
                DeclareTransactionV0V1 { class_hash, .. },
                _,
                _,
            ) => Some(*class_hash),
            _ => None,
        };
        let blockifier_tx = to_blockifier_tx(tx, tx_hash, transaction_index)?;
        let tx_execution_info_result =
            blockifier_tx.execute(&mut transactional_state, &block_context, charge_fee, validate);
        let state_diff =
            induced_state_diff(&mut transactional_state, deprecated_declared_class_hash)?;
        transactional_state.commit();
        let execution_info = tx_execution_info_result.map_err(|error| {
            if let Some(class_hash) = cached_state.state.missing_compiled_class {
                ExecutionError::MissingCompiledClass { class_hash }
            } else {
                ExecutionError::from((transaction_index, error))
            }
        })?;
        res.push(TransactionExecutionOutput {
            execution_info,
            induced_state_diff: state_diff,
            price_unit,
        });
    }

    Ok((res, block_context))
}

/// Converts a transaction index and [BlockifierTransactionExecutionError] to an [ExecutionError].
// TODO(yair): Remove once blockifier arranges the errors hierarchy.
impl From<(usize, BlockifierTransactionExecutionError)> for ExecutionError {
    fn from(transaction_index_and_error: (usize, BlockifierTransactionExecutionError)) -> Self {
        let (transaction_index, error) = transaction_index_and_error;
        Self::TransactionExecutionError { transaction_index, execution_error: error.to_string() }
    }
}

/// Sets the block hash contract (contract at address 1) with the block hash of the block 10 blocks
/// ago.
fn set_block_hash_contract(
    state_number: StateNumber,
    cached_state: &mut CachedState<ExecutionStateReader>,
) -> ExecutionResult<()> {
    if state_number.is_after(BlockNumber(10)) {
        let block_min_10 = state_number.0.0 - 10;
        let header_10_blocks_ago = cached_state
            .state
            .storage_reader
            .begin_ro_txn()?
            .get_block_header(BlockNumber(block_min_10))?
            .expect("State should be > 10.");
        // TODO: check this result
        let _ = cached_state.set_storage_at(
            ContractAddress::from(1_u128),
            StorageKey::from(block_min_10),
            header_10_blocks_ago.block_hash.0,
        );
    }
    Ok(())
}

fn to_blockifier_tx(
    tx: ExecutableTransactionInput,
    tx_hash: TransactionHash,
    transaction_index: usize,
) -> ExecutionResult<BlockifierTransaction> {
    // TODO(yair): support only_query version bit (enable in the RPC v0.6 and use the correct
    // value).
    match tx {
        ExecutableTransactionInput::Invoke(invoke_tx, only_query) => {
            BlockifierTransaction::from_api(
                Transaction::Invoke(invoke_tx),
                tx_hash,
                None,
                None,
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }

        ExecutableTransactionInput::DeployAccount(deploy_acc_tx, only_query) => {
            BlockifierTransaction::from_api(
                Transaction::DeployAccount(deploy_acc_tx),
                tx_hash,
                None,
                None,
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }

        ExecutableTransactionInput::DeclareV0(declare_tx, deprecated_class, only_query) => {
            let class_v0 = BlockifierContractClass::V0(deprecated_class.try_into().map_err(
                |e: cairo_vm::types::errors::program_errors::ProgramError| {
                    ExecutionError::TransactionExecutionError {
                        transaction_index,
                        execution_error: e.to_string(),
                    }
                },
            )?);
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V0(declare_tx)),
                tx_hash,
                Some(class_v0),
                None,
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::DeclareV1(declare_tx, deprecated_class, only_query) => {
            let class_v0 = BlockifierContractClass::V0(
                deprecated_class.try_into().map_err(BlockifierError::new)?,
            );
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V1(declare_tx)),
                tx_hash,
                Some(class_v0),
                None,
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::DeclareV2(declare_tx, compiled_class, only_query) => {
            let class_v1 = BlockifierContractClass::V1(
                compiled_class.try_into().map_err(BlockifierError::new)?,
            );
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V2(declare_tx)),
                tx_hash,
                Some(class_v1),
                None,
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::DeclareV3(declare_tx, compiled_class, only_query) => {
            let class_v1 = BlockifierContractClass::V1(
                compiled_class.try_into().map_err(BlockifierError::new)?,
            );
            BlockifierTransaction::from_api(
                Transaction::Declare(DeclareTransaction::V3(declare_tx)),
                tx_hash,
                Some(class_v1),
                None,
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
        }
        ExecutableTransactionInput::L1Handler(l1_handler_tx, paid_fee, only_query) => {
            BlockifierTransaction::from_api(
                Transaction::L1Handler(l1_handler_tx),
                tx_hash,
                None,
                Some(paid_fee),
                None,
                only_query,
            )
            .map_err(|err| ExecutionError::from((transaction_index, err)))
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
) -> ExecutionResult<Vec<TransactionSimulationOutput>> {
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
    execution_results
        .into_iter()
        .zip(trace_constructors)
        .map(|(tx_execution_output, trace_constructor)| {
            let fee = tx_execution_output.execution_info.actual_fee;
            let gas_price = match tx_execution_output.price_unit {
                PriceUnit::Wei => GasPrice(block_context.block_info.gas_prices.eth_l1_gas_price),
                PriceUnit::Fri => GasPrice(block_context.block_info.gas_prices.strk_l1_gas_price),
            };
            match trace_constructor(tx_execution_output.execution_info) {
                Ok(transaction_trace) => Ok(TransactionSimulationOutput {
                    transaction_trace,
                    induced_state_diff: tx_execution_output.induced_state_diff,
                    gas_price,
                    fee,
                    price_unit: tx_execution_output.price_unit,
                }),
                Err(e) => Err(e),
            }
        })
        .collect()
}
