use blockifier::execution::entry_point::{
    CallInfo, CallType as BlockifierCallType, OrderedEvent as BlockifierOrderedEvent,
    OrderedL2ToL1Message as BlockifierOrderedL2ToL1Message, Retdata as BlockifierRetdata,
};
use blockifier::transaction::objects::TransactionExecutionInfo;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{Calldata, EventContent, MessageToL1};

// TODO(yair): Move to SN_API? need this in the GW.
#[derive(Serialize, Deserialize)]
pub enum TransactionTrace {
    Invoke(InvokeTransactionTrace),
    Declare(DeclareTransactionTrace),
    DeployAccount(DeployAccountTransactionTrace),
    L1Handler(L1HandlerTransactionTrace),
}

#[derive(Serialize, Deserialize)]
pub struct InvokeTransactionTrace {
    pub validate_invocation: Option<FunctionInvocation>,
    pub execute_invocation: FunctionInvocation,
    pub fee_transfer_invocation: Option<FunctionInvocation>,
}

impl From<TransactionExecutionInfo> for InvokeTransactionTrace {
    fn from(transaction_execution_info: TransactionExecutionInfo) -> Self {
        Self {
            validate_invocation: transaction_execution_info
                .validate_call_info
                .map(FunctionInvocation::from),
            execute_invocation: transaction_execution_info
                .execute_call_info
                .expect("Invoke execution should contain execute call info")
                .into(),
            fee_transfer_invocation: transaction_execution_info
                .fee_transfer_call_info
                .map(FunctionInvocation::from),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeclareTransactionTrace {
    validate_invocation: Option<FunctionInvocation>,
    fee_transfer_invocation: Option<FunctionInvocation>,
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

#[derive(Serialize, Deserialize)]
pub struct DeployAccountTransactionTrace {
    validate_invocation: Option<FunctionInvocation>,
    constructor_invocation: FunctionInvocation,
    fee_transfer_invocation: Option<FunctionInvocation>,
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

#[derive(Serialize, Deserialize)]
pub struct L1HandlerTransactionTrace {
    function_invocation: FunctionInvocation,
}

#[derive(Serialize, Deserialize)]
pub struct FunctionInvocation {
    #[serde(flatten)]
    function_call: FunctionCall,
    caller_address: ContractAddress,
    class_hash: ClassHash,
    entry_point_type: EntryPointType,
    // TODO(yair): check the serialization of this field.
    call_type: CallType,
    result: Retdata,
    // Box?
    calls: Vec<Self>,
    // TODO(yair): Fix the RPC to array.
    pub events: Vec<OrderedEvent>,
    pub messages: Vec<OrderedL2ToL1Message>,
}

impl From<CallInfo> for FunctionInvocation {
    fn from(call_info: CallInfo) -> Self {
        Self {
            function_call: FunctionCall {
                contract_address: call_info.call.code_address.unwrap(), // TODO: fix this.
                entry_point_selector: call_info.call.entry_point_selector,
                calldata: call_info.call.calldata,
            },
            caller_address: call_info.call.caller_address,
            class_hash: call_info.call.class_hash.unwrap(), // TODO: fix this.
            entry_point_type: call_info.call.entry_point_type,
            call_type: call_info.call.call_type.into(),
            result: call_info.execution.retdata.into(),
            calls: call_info.inner_calls.into_iter().map(Self::from).collect(),
            events: call_info.execution.events.into_iter().map(Into::into).collect(),
            messages: call_info
                .execution
                .l2_to_l1_messages
                .into_iter()
                .map(|blockifier_message| {
                    OrderedL2ToL1Message::from(
                        blockifier_message,
                        call_info.call.code_address.unwrap(), // TODO: fix this.
                    )
                })
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum CallType {
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Retdata(pub Vec<StarkFelt>);

impl From<BlockifierRetdata> for Retdata {
    fn from(retdata: BlockifierRetdata) -> Self {
        Self(retdata.0)
    }
}

#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderedEvent {
    pub order: usize,
    pub event: EventContent,
}

impl From<BlockifierOrderedEvent> for OrderedEvent {
    fn from(ordered_event: BlockifierOrderedEvent) -> Self {
        Self { order: ordered_event.order, event: ordered_event.event }
    }
}

#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderedL2ToL1Message {
    pub order: usize,
    pub message: MessageToL1,
}

impl OrderedL2ToL1Message {
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

#[derive(Serialize, Deserialize)]
pub struct FunctionCall {
    contract_address: ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
}
