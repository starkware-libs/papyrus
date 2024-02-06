//! Execution objects.
use std::collections::HashMap;

use blockifier::execution::call_info::{
    CallInfo,
    OrderedEvent as BlockifierOrderedEvent,
    OrderedL2ToL1Message as BlockifierOrderedL2ToL1Message,
    Retdata as BlockifierRetdata,
};
use blockifier::execution::entry_point::CallType as BlockifierCallType;
use blockifier::transaction::objects::TransactionExecutionInfo;
use cairo_vm::vm::runners::builtin_runner::{
    BITWISE_BUILTIN_NAME,
    EC_OP_BUILTIN_NAME,
    HASH_BUILTIN_NAME,
    KECCAK_BUILTIN_NAME,
    OUTPUT_BUILTIN_NAME,
    POSEIDON_BUILTIN_NAME,
    RANGE_CHECK_BUILTIN_NAME,
    SEGMENT_ARENA_BUILTIN_NAME,
    SIGNATURE_BUILTIN_NAME,
};
use cairo_vm::vm::runners::cairo_runner::ExecutionResources as VmExecutionResources;
use indexmap::IndexMap;
use itertools::Itertools;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_common::state::{
    DeclaredClassHashEntry,
    DeployedContract,
    ReplacedClass,
    StorageEntry,
};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockTimestamp, GasPrice};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::{
    Builtin,
    Calldata,
    EventContent,
    ExecutionResources,
    Fee,
    MessageToL1,
};
use starknet_types_core::felt::Felt;

use crate::{ExecutionError, ExecutionResult};

// TODO(yair): Move types to starknet_api.

/// The output of simulating a transaction.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TransactionSimulationOutput {
    /// The execution trace of the transaction.
    pub transaction_trace: TransactionTrace,
    /// The state diff induced by the transaction.
    pub induced_state_diff: ThinStateDiff,
    /// The gas price in the block context of the transaction execution.
    pub gas_price: GasPrice,
    /// The fee in the block context of the transaction execution.
    pub fee: Fee,
    /// The unit of the fee.
    pub price_unit: PriceUnit,
}

/// The execution trace of a transaction.
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum TransactionTrace {
    #[serde(rename = "L1_HANDLER")]
    L1Handler(L1HandlerTransactionTrace),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransactionTrace),
    #[serde(rename = "DECLARE")]
    Declare(DeclareTransactionTrace),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransactionTrace),
}

/// The execution trace of an Invoke transaction.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct InvokeTransactionTrace {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The trace of the __validate__ call.
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __execute__ call or the reason in case of reverted transaction.
    pub execute_invocation: FunctionInvocationResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The trace of the __fee_transfer__ call.
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

/// The reason for a reverted transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum RevertReason {
    #[serde(rename = "revert_reason")]
    RevertReason(String),
}

impl TryFrom<TransactionExecutionInfo> for InvokeTransactionTrace {
    type Error = ExecutionError;
    fn try_from(transaction_execution_info: TransactionExecutionInfo) -> ExecutionResult<Self> {
        let execute_invocation = match transaction_execution_info.revert_error {
            Some(revert_error) => {
                FunctionInvocationResult::Err(RevertReason::RevertReason(revert_error))
            }
            None => FunctionInvocationResult::Ok(
                transaction_execution_info
                    .execute_call_info
                    .expect("Invoke transaction execution should contain execute_call_info.")
                    .try_into()?,
            ),
        };

        Ok(Self {
            validate_invocation: match transaction_execution_info.validate_call_info {
                None => None,
                Some(call_info) => Some(call_info.try_into()?),
            },
            execute_invocation,
            fee_transfer_invocation: match transaction_execution_info.fee_transfer_call_info {
                None => None,
                Some(call_info) => Some(call_info.try_into()?),
            },
        })
    }
}

/// The execution trace of a Declare transaction.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct DeclareTransactionTrace {
    /// The trace of the __validate__ call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __fee_transfer__ call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

impl TryFrom<TransactionExecutionInfo> for DeclareTransactionTrace {
    type Error = ExecutionError;
    fn try_from(transaction_execution_info: TransactionExecutionInfo) -> ExecutionResult<Self> {
        Ok(Self {
            validate_invocation: match transaction_execution_info.validate_call_info {
                None => None,
                Some(call_info) => Some(call_info.try_into()?),
            },
            fee_transfer_invocation: match transaction_execution_info.fee_transfer_call_info {
                None => None,
                Some(call_info) => Some(call_info.try_into()?),
            },
        })
    }
}

/// The execution trace of a DeployAccount transaction.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct DeployAccountTransactionTrace {
    /// The trace of the __validate__ call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __constructor__ call.
    pub constructor_invocation: FunctionInvocation,
    /// The trace of the __fee_transfer__ call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

impl TryFrom<TransactionExecutionInfo> for DeployAccountTransactionTrace {
    type Error = ExecutionError;
    fn try_from(transaction_execution_info: TransactionExecutionInfo) -> ExecutionResult<Self> {
        Ok(Self {
            validate_invocation: match transaction_execution_info.validate_call_info {
                None => None,
                Some(call_info) => Some(call_info.try_into()?),
            },
            constructor_invocation: transaction_execution_info
                .execute_call_info
                .expect(
                    "Deploy account execution should contain execute_call_info (the constructor \
                     call info).",
                )
                .try_into()?,
            fee_transfer_invocation: match transaction_execution_info.fee_transfer_call_info {
                None => None,
                Some(call_info) => Some(call_info.try_into()?),
            },
        })
    }
}

/// The execution trace of an L1Handler transaction.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct L1HandlerTransactionTrace {
    /// The trace of the funcion call.
    pub function_invocation: FunctionInvocation,
}

impl TryFrom<TransactionExecutionInfo> for L1HandlerTransactionTrace {
    type Error = ExecutionError;
    fn try_from(transaction_execution_info: TransactionExecutionInfo) -> ExecutionResult<Self> {
        Ok(Self {
            function_invocation: transaction_execution_info
                .execute_call_info
                .expect("L1Handler execution should contain execute_call_info.")
                .try_into()?,
        })
    }
}

/// Wether the function invocation succeeded or reverted.
// Not using `Result` because it is not being serialized according to the spec.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[allow(missing_docs)]
#[serde(untagged)]
pub enum FunctionInvocationResult {
    Ok(FunctionInvocation),
    Err(RevertReason),
}

/// The execution trace of a function call.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct FunctionInvocation {
    #[serde(flatten)]
    /// The details of the function call.
    pub function_call: FunctionCall,
    /// The address of the invoking contract. 0 for the root invocation.
    pub caller_address: ContractAddress,
    /// The hash of the class being called.
    pub class_hash: ClassHash,
    /// The type of the entry point being called.
    pub entry_point_type: EntryPointType,
    /// library call or regular call.
    pub call_type: CallType,
    /// The value returned from the function invocation.
    pub result: Retdata,
    /// The calls made by this invocation.
    pub calls: Vec<Self>,
    /// The events emitted in this invocation.
    pub events: Vec<OrderedEvent>,
    /// The messages sent by this invocation to L1.
    pub messages: Vec<OrderedL2ToL1Message>,
    /// The VM execution resources used by this invocation.
    pub execution_resources: ExecutionResources,
}

impl TryFrom<CallInfo> for FunctionInvocation {
    type Error = ExecutionError;
    fn try_from(call_info: CallInfo) -> ExecutionResult<Self> {
        Ok(Self {
            function_call: FunctionCall {
                contract_address: call_info.call.storage_address,
                entry_point_selector: call_info.call.entry_point_selector,
                calldata: call_info.call.calldata,
            },
            caller_address: call_info.call.caller_address,
            class_hash: call_info.call.class_hash.ok_or(ExecutionError::MissingClassHash)?, /* TODO: fix this. */
            entry_point_type: call_info.call.entry_point_type,
            call_type: call_info.call.call_type.into(),
            result: call_info.execution.retdata.into(),
            calls: call_info
                .inner_calls
                .into_iter()
                .map(Self::try_from)
                .collect::<Result<_, _>>()?,
            events: call_info
                .execution
                .events
                .into_iter()
                .sorted_by_key(|ordered_event| ordered_event.order)
                .map(OrderedEvent::from)
                .collect(),
            messages: call_info
                .execution
                .l2_to_l1_messages
                .into_iter()
                .sorted_by_key(|ordered_message| ordered_message.order)
                .map(|ordered_message| {
                    // TODO(yair): write a test that verifies that the from_address is correct.
                    OrderedL2ToL1Message::from(ordered_message, call_info.call.storage_address)
                })
                .collect(),
            execution_resources: vm_resources_to_execution_resources(call_info.vm_resources)?,
        })
    }
}

// Can't implement `TryFrom` because both types are from external crates.
fn vm_resources_to_execution_resources(
    vm_resources: VmExecutionResources,
) -> ExecutionResult<ExecutionResources> {
    let mut builtin_instance_counter = HashMap::new();
    for (builtin_name, count) in vm_resources.builtin_instance_counter {
        if count == 0 {
            continue;
        }
        let count: u64 = count as u64;
        match builtin_name.as_str() {
            OUTPUT_BUILTIN_NAME => {
                continue;
            }
            HASH_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::Pedersen, count),
            RANGE_CHECK_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::RangeCheck, count),
            SIGNATURE_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::Ecdsa, count),
            BITWISE_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::Bitwise, count),
            EC_OP_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::EcOp, count),
            KECCAK_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::Keccak, count),
            POSEIDON_BUILTIN_NAME => builtin_instance_counter.insert(Builtin::Poseidon, count),
            SEGMENT_ARENA_BUILTIN_NAME => {
                builtin_instance_counter.insert(Builtin::SegmentArena, count)
            }
            _ => {
                return Err(ExecutionError::UnknownBuiltin { builtin_name });
            }
        };
    }
    Ok(ExecutionResources {
        steps: vm_resources.n_steps as u64,
        builtin_instance_counter,
        memory_holes: vm_resources.n_memory_holes as u64,
    })
}

/// library call or regular call.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[allow(missing_docs)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CallType {
    Call,
    LibraryCall,
}

impl From<BlockifierCallType> for CallType {
    fn from(call_type: BlockifierCallType) -> Self {
        match call_type {
            BlockifierCallType::Call => CallType::Call,
            BlockifierCallType::Delegate => CallType::LibraryCall,
        }
    }
}

/// The return data of a function call.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Retdata(pub Vec<Felt>);

impl From<BlockifierRetdata> for Retdata {
    fn from(retdata: BlockifierRetdata) -> Self {
        Self(retdata.0)
    }
}

/// An event emitted by a contract.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderedEvent {
    /// The order of the event in the transaction.
    pub order: usize,
    #[serde(flatten)]
    /// The event.
    pub event: EventContent,
}

impl From<BlockifierOrderedEvent> for OrderedEvent {
    fn from(ordered_event: BlockifierOrderedEvent) -> Self {
        Self { order: ordered_event.order, event: ordered_event.event }
    }
}

/// A message sent from L2 to L1.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderedL2ToL1Message {
    /// The order of the message in the transaction.
    pub order: usize,
    #[serde(flatten)]
    /// The message.
    pub message: MessageToL1,
}

impl OrderedL2ToL1Message {
    /// Constructs a new `OrderedL2ToL1Message`.
    pub fn from(
        blockifier_message: BlockifierOrderedL2ToL1Message,
        from_address: ContractAddress,
    ) -> Self {
        Self {
            order: blockifier_message.order,
            message: MessageToL1 {
                from_address,
                to_address: blockifier_message.message.to_address,
                payload: blockifier_message.message.payload,
            },
        }
    }
}

/// The details of a function call.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct FunctionCall {
    /// The address of the contract being called.
    pub contract_address: ContractAddress,
    /// The selector of the entry point being called.
    pub entry_point_selector: EntryPointSelector,
    /// The calldata of the function call.
    pub calldata: Calldata,
}

/// A state diff for the pending block.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct PendingData {
    // TODO(shahak): Consider indexing by address and key.
    /// All the contract storages that were changed in the pending block.
    pub storage_diffs: IndexMap<ContractAddress, Vec<StorageEntry>>,
    /// All the contracts that were deployed in the pending block.
    pub deployed_contracts: Vec<DeployedContract>,
    /// All the classes that were declared in the pending block.
    pub declared_classes: Vec<DeclaredClassHashEntry>,
    /// All the deprecated classes that were declared in the pending block.
    pub old_declared_contracts: Vec<ClassHash>,
    /// All the nonces that were changed in the pending block.
    pub nonces: IndexMap<ContractAddress, Nonce>,
    /// All the classes that were declared in the pending block.
    pub replaced_classes: Vec<ReplacedClass>,
    /// The timestamp of the pending block.
    pub timestamp: BlockTimestamp,
    /// The ETH gas price of the pending block.
    pub eth_l1_gas_price: GasPrice,
    /// The STRK gas price of the pending block.
    pub strk_l1_gas_price: GasPrice,
    /// The sequencer address of the pending block.
    pub sequencer: ContractAddress,
    /// The classes and casms that were declared in the pending block.
    pub classes: PendingClasses,
}

/// The unit of the fee.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PriceUnit {
    /// Wei.
    #[default]
    Wei,
    /// Fri.
    Fri,
}
