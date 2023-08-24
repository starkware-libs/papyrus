//! Execution objects.

use blockifier::execution::entry_point::{
    CallInfo,
    CallType as BlockifierCallType,
    OrderedEvent as BlockifierOrderedEvent,
    OrderedL2ToL1Message as BlockifierOrderedL2ToL1Message,
    Retdata as BlockifierRetdata,
};
use blockifier::transaction::objects::TransactionExecutionInfo;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{Calldata, EventContent, MessageToL1};

/// The execution trace of a transaction.
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransactionTrace {
    L1Handler(L1HandlerTransactionTrace),
    Invoke(InvokeTransactionTrace),
    Declare(DeclareTransactionTrace),
    DeployAccount(DeployAccountTransactionTrace),
}

/// The execution trace of an Invoke transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvokeTransactionTrace {
    /// The trace of the __validate__ call.
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __execute__ call or the reason in case of reverted transaction.
    pub execute_invocation: FunctionInvocationResult,
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

impl From<TransactionExecutionInfo> for InvokeTransactionTrace {
    fn from(transaction_execution_info: TransactionExecutionInfo) -> Self {
        let execute_invocation = match transaction_execution_info.revert_error {
            Some(revert_error) => {
                FunctionInvocationResult::Err(RevertReason::RevertReason(revert_error))
            }
            None => FunctionInvocationResult::Ok(
                transaction_execution_info
                    .execute_call_info
                    .expect("Invoke transaction execution should contain execute_call_info.")
                    .into(),
            ),
        };

        Self {
            validate_invocation: transaction_execution_info
                .validate_call_info
                .map(FunctionInvocation::from),
            execute_invocation,
            fee_transfer_invocation: transaction_execution_info
                .fee_transfer_call_info
                .map(FunctionInvocation::from),
        }
    }
}

/// The execution trace of a Declare transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeclareTransactionTrace {
    /// The trace of the __validate__ call.
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __fee_transfer__ call.
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

impl From<TransactionExecutionInfo> for DeclareTransactionTrace {
    fn from(transaction_execution_info: TransactionExecutionInfo) -> Self {
        Self {
            validate_invocation: transaction_execution_info
                .validate_call_info
                .map(FunctionInvocation::from),
            fee_transfer_invocation: transaction_execution_info
                .fee_transfer_call_info
                .map(FunctionInvocation::from),
        }
    }
}

/// The execution trace of a DeployAccount transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeployAccountTransactionTrace {
    /// The trace of the __validate__ call.
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __constructor__ call.
    pub constructor_invocation: FunctionInvocation,
    /// The trace of the __fee_transfer__ call.
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

impl From<TransactionExecutionInfo> for DeployAccountTransactionTrace {
    fn from(transaction_execution_info: TransactionExecutionInfo) -> Self {
        Self {
            validate_invocation: transaction_execution_info
                .validate_call_info
                .map(FunctionInvocation::from),
            constructor_invocation: transaction_execution_info
                .execute_call_info
                .expect(
                    "Deploy account execution should contain execute_call_info (the constructor \
                     call info).",
                )
                .into(),
            fee_transfer_invocation: transaction_execution_info
                .fee_transfer_call_info
                .map(FunctionInvocation::from),
        }
    }
}

/// The execution trace of an L1Handler transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct L1HandlerTransactionTrace {
    /// The trace of the funcion call.
    pub function_invocation: FunctionInvocation,
}

impl From<TransactionExecutionInfo> for L1HandlerTransactionTrace {
    fn from(transaction_execution_info: TransactionExecutionInfo) -> Self {
        Self {
            function_invocation: transaction_execution_info
                .execute_call_info
                .expect("L1Handler execution should contain execute_call_info.")
                .into(),
        }
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
    pub events: Vec<EventContent>,
    /// The messages sent by this invocation to L1.
    pub messages: Vec<MessageToL1>,
}

impl From<CallInfo> for FunctionInvocation {
    fn from(call_info: CallInfo) -> Self {
        Self {
            function_call: FunctionCall {
                contract_address: call_info.call.storage_address,
                entry_point_selector: call_info.call.entry_point_selector,
                calldata: call_info.call.calldata,
            },
            caller_address: call_info.call.caller_address,
            class_hash: call_info.call.class_hash.unwrap(), // TODO: fix this.
            entry_point_type: call_info.call.entry_point_type,
            call_type: call_info.call.call_type.into(),
            result: call_info.execution.retdata.into(),
            calls: call_info.inner_calls.into_iter().map(Self::from).collect(),
            events: call_info
                .execution
                .events
                .into_iter()
                .sorted_by_key(|ordered_event| ordered_event.order)
                .map(|ordered_event| ordered_event.event)
                .collect(),
            messages: call_info
                .execution
                .l2_to_l1_messages
                .into_iter()
                .sorted_by_key(|ordered_message| ordered_message.order)
                .map(|ordered_message| MessageToL1 {
                    from_address: call_info.call.caller_address,
                    to_address: ordered_message.message.to_address,
                    payload: ordered_message.message.payload,
                })
                .collect(),
        }
    }
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
pub struct Retdata(pub Vec<StarkFelt>);

impl From<BlockifierRetdata> for Retdata {
    fn from(retdata: BlockifierRetdata) -> Self {
        Self(retdata.0)
    }
}

/// An event emitted by a contract.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderedEvent {
    /// The order of the event in the transaction.
    #[serde(skip)]
    pub order: usize,
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
