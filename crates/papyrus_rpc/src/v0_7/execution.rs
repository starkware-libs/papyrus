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
use starknet_api::state::ThinStateDiff as StarknetApiThinStateDiff;

use super::state::ThinStateDiff;
use super::transaction::{ComputationResources, ExecutionResources};

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
    /// The state diff induced by this transaction.
    pub state_diff: ThinStateDiff,
    /// The total execution resources of this transaction.
    pub execution_resources: ExecutionResources,
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
    /// The state diff induced by this transaction.
    pub state_diff: ThinStateDiff,
    /// The total execution resources of this transaction.
    pub execution_resources: ExecutionResources,
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
    /// The state diff induced by this transaction.
    pub state_diff: ThinStateDiff,
    /// The total execution resources of this transaction.
    pub execution_resources: ExecutionResources,
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
    /// The state diff induced by this transaction.
    pub state_diff: ThinStateDiff,
    /// The total execution resources of this transaction.
    pub execution_resources: ExecutionResources,
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
    pub execution_resources: ComputationResources,
}

impl From<(ExecutionTransactionTrace, StarknetApiThinStateDiff)> for TransactionTrace {
    fn from((trace, state_diff): (ExecutionTransactionTrace, StarknetApiThinStateDiff)) -> Self {
        let mut state_diff = ThinStateDiff::from(state_diff);
        // TODO: Investigate why blockifier sometimes returns unsorted state diff
        state_diff.sort();
        match trace {
            ExecutionTransactionTrace::L1Handler(trace) => {
                let execution_resources =
                    trace.function_invocation.execution_resources.clone().into();
                Self::L1Handler(L1HandlerTransactionTrace {
                    function_invocation: trace.function_invocation.into(),
                    state_diff,
                    execution_resources,
                })
            }
            ExecutionTransactionTrace::Invoke(trace) => {
                let (execute_invocation, execute_execution_resources) = match trace
                    .execute_invocation
                {
                    ExecutionFunctionInvocationResult::Ok(function_invocation) => {
                        let execution_resources = function_invocation.execution_resources.clone();
                        (
                            FunctionInvocationResult::Ok(function_invocation.into()),
                            ExecutionResources::from(execution_resources),
                        )
                    }
                    ExecutionFunctionInvocationResult::Err(revert_reason) => (
                        FunctionInvocationResult::Err(revert_reason),
                        ExecutionResources::default(),
                    ),
                };

                let validate_execution_resources =
                    trace.validate_invocation.as_ref().map(|invocation| {
                        ExecutionResources::from(invocation.execution_resources.clone())
                    });
                let fee_transfer_execution_resources =
                    trace.fee_transfer_invocation.as_ref().map(|invocation| {
                        ExecutionResources::from(invocation.execution_resources.clone())
                    });

                Self::Invoke(InvokeTransactionTrace {
                    validate_invocation: trace.validate_invocation.map(Into::into),
                    execute_invocation,
                    fee_transfer_invocation: trace.fee_transfer_invocation.map(Into::into),
                    state_diff,
                    execution_resources: validate_execution_resources.unwrap_or_default()
                        + execute_execution_resources
                        + fee_transfer_execution_resources.unwrap_or_default(),
                })
            }
            ExecutionTransactionTrace::Declare(trace) => {
                let validate_execution_resources =
                    trace.validate_invocation.as_ref().map(|invocation| {
                        ExecutionResources::from(invocation.execution_resources.clone())
                    });
                let fee_transfer_execution_resources =
                    trace.fee_transfer_invocation.as_ref().map(|invocation| {
                        ExecutionResources::from(invocation.execution_resources.clone())
                    });
                Self::Declare(DeclareTransactionTrace {
                    validate_invocation: trace.validate_invocation.map(Into::into),
                    fee_transfer_invocation: trace.fee_transfer_invocation.map(Into::into),
                    state_diff,
                    execution_resources: validate_execution_resources.unwrap_or_default()
                        + fee_transfer_execution_resources.unwrap_or_default(),
                })
            }
            ExecutionTransactionTrace::DeployAccount(trace) => {
                let validate_execution_resources =
                    trace.validate_invocation.as_ref().map(|invocation| {
                        ExecutionResources::from(invocation.execution_resources.clone())
                    });
                let constructor_execution_resources = ExecutionResources::from(
                    trace.constructor_invocation.execution_resources.clone(),
                );
                let fee_transfer_execution_resources =
                    trace.fee_transfer_invocation.as_ref().map(|invocation| {
                        ExecutionResources::from(invocation.execution_resources.clone())
                    });
                Self::DeployAccount(DeployAccountTransactionTrace {
                    validate_invocation: trace.validate_invocation.map(Into::into),
                    constructor_invocation: trace.constructor_invocation.into(),
                    fee_transfer_invocation: trace.fee_transfer_invocation.map(Into::into),
                    state_diff,
                    execution_resources: validate_execution_resources.unwrap_or_default()
                        + constructor_execution_resources
                        + fee_transfer_execution_resources.unwrap_or_default(),
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
            execution_resources: invocation.execution_resources.into(),
        }
    }
}
