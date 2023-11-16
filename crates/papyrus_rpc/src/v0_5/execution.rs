use papyrus_execution::objects::{CallType, FunctionCall, Retdata, RevertReason};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::transaction::{EventContent, MessageToL1};

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

impl From<papyrus_execution::objects::TransactionTrace> for TransactionTrace {
    fn from(value: papyrus_execution::objects::TransactionTrace) -> Self {
        match value {
            papyrus_execution::objects::TransactionTrace::L1Handler(l1_handler) => {
                Self::L1Handler(l1_handler.into())
            }
            papyrus_execution::objects::TransactionTrace::Invoke(invoke) => {
                Self::Invoke(invoke.into())
            }
            papyrus_execution::objects::TransactionTrace::Declare(declare) => {
                Self::Declare(declare.into())
            }
            papyrus_execution::objects::TransactionTrace::DeployAccount(deploy_account) => {
                Self::DeployAccount(deploy_account.into())
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct L1HandlerTransactionTrace {
    /// The trace of the funcion call.
    pub function_invocation: FunctionInvocation,
}

impl From<papyrus_execution::objects::L1HandlerTransactionTrace> for L1HandlerTransactionTrace {
    fn from(value: papyrus_execution::objects::L1HandlerTransactionTrace) -> Self {
        Self { function_invocation: value.function_invocation.into() }
    }
}

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

impl From<papyrus_execution::objects::InvokeTransactionTrace> for InvokeTransactionTrace {
    fn from(value: papyrus_execution::objects::InvokeTransactionTrace) -> Self {
        Self {
            validate_invocation: value.validate_invocation.map(Into::into),
            execute_invocation: match value.execute_invocation {
                papyrus_execution::objects::FunctionInvocationResult::Ok(function_invocation) => {
                    FunctionInvocationResult::Ok(function_invocation.into())
                }
                papyrus_execution::objects::FunctionInvocationResult::Err(revert_reason) => {
                    FunctionInvocationResult::Err(revert_reason)
                }
            },
            fee_transfer_invocation: value.fee_transfer_invocation.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct DeclareTransactionTrace {
    /// The trace of the __validate__ call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_invocation: Option<FunctionInvocation>,
    /// The trace of the __fee_transfer__ call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

impl From<papyrus_execution::objects::DeclareTransactionTrace> for DeclareTransactionTrace {
    fn from(value: papyrus_execution::objects::DeclareTransactionTrace) -> Self {
        Self {
            validate_invocation: value.validate_invocation.map(Into::into),
            fee_transfer_invocation: value.fee_transfer_invocation.map(Into::into),
        }
    }
}

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

impl From<papyrus_execution::objects::DeployAccountTransactionTrace>
    for DeployAccountTransactionTrace
{
    fn from(value: papyrus_execution::objects::DeployAccountTransactionTrace) -> Self {
        Self {
            validate_invocation: value.validate_invocation.map(Into::into),
            constructor_invocation: value.constructor_invocation.into(),
            fee_transfer_invocation: value.fee_transfer_invocation.map(Into::into),
        }
    }
}

// Note: in 0.4.0 the eventes and messages shouldn't have the order field.
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

impl From<papyrus_execution::objects::FunctionInvocation> for FunctionInvocation {
    fn from(value: papyrus_execution::objects::FunctionInvocation) -> Self {
        Self {
            function_call: value.function_call,
            caller_address: value.caller_address,
            class_hash: value.class_hash,
            entry_point_type: value.entry_point_type,
            call_type: value.call_type,
            result: value.result,
            calls: value.calls.into_iter().map(Self::from).collect(),
            events: value.events.into_iter().map(|ordered_event| ordered_event.event).collect(),
            messages: value
                .messages
                .into_iter()
                .map(|ordered_message| ordered_message.message)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum FunctionInvocationResult {
    Ok(FunctionInvocation),
    Err(RevertReason),
}
