use papyrus_execution::objects::{
    CallType,
    FunctionCall,
    FunctionInvocation as ExecutionFunctionInvocation,
    FunctionInvocationResult as ExecutionFunctionInvocationResult,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
    RevertReason,
    TransactionTrace as ExecutionTransactionTrace,
};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::deprecated_contract_class::EntryPointType;

// The only difference between this and TransactionTrace in the execution crate is the
// ExecutionResources inside FunctionInvocation.
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

/// The execution trace of an L1Handler transaction.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct L1HandlerTransactionTrace {
    /// The trace of the funcion call.
    pub function_invocation: FunctionInvocation,
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
}

impl From<ExecutionTransactionTrace> for TransactionTrace {
    fn from(value: ExecutionTransactionTrace) -> Self {
        match value {
            ExecutionTransactionTrace::L1Handler(trace) => {
                Self::L1Handler(L1HandlerTransactionTrace {
                    function_invocation: trace.function_invocation.into(),
                })
            }
            ExecutionTransactionTrace::Invoke(trace) => {
                let execute_invocation = match trace.execute_invocation {
                    ExecutionFunctionInvocationResult::Ok(function_invocation) => {
                        FunctionInvocationResult::Ok(function_invocation.into())
                    }
                    ExecutionFunctionInvocationResult::Err(revert_reason) => {
                        FunctionInvocationResult::Err(revert_reason)
                    }
                };

                Self::Invoke(InvokeTransactionTrace {
                    validate_invocation: trace.validate_invocation.map(Into::into),
                    execute_invocation,
                    fee_transfer_invocation: trace.fee_transfer_invocation.map(Into::into),
                })
            }
            ExecutionTransactionTrace::Declare(trace) => Self::Declare(DeclareTransactionTrace {
                validate_invocation: trace.validate_invocation.map(Into::into),
                fee_transfer_invocation: trace.fee_transfer_invocation.map(Into::into),
            }),
            ExecutionTransactionTrace::DeployAccount(trace) => {
                Self::DeployAccount(DeployAccountTransactionTrace {
                    validate_invocation: trace.validate_invocation.map(Into::into),
                    constructor_invocation: trace.constructor_invocation.into(),
                    fee_transfer_invocation: trace.fee_transfer_invocation.map(Into::into),
                })
            }
        }
    }
}

impl From<ExecutionFunctionInvocation> for FunctionInvocation {
    fn from(invocation: ExecutionFunctionInvocation) -> Self {
        Self {
            function_call: invocation.function_call,
            caller_address: invocation.caller_address,
            class_hash: invocation.class_hash,
            entry_point_type: invocation.entry_point_type,
            call_type: invocation.call_type,
            result: invocation.result,
            calls: invocation.calls.into_iter().map(Into::into).collect(),
            events: invocation.events,
            messages: invocation.messages,
        }
    }
}
