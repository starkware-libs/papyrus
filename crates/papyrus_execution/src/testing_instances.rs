#![allow(clippy::unwrap_used)]
//! Utilities for generating testing instances of the execution objects.
use std::path::PathBuf;

/// Returns the storage key of a storage variable.
pub use blockifier::abi::abi_utils::get_storage_var_address;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, PatriciaKey};
use starknet_api::deprecated_contract_class::EntryPointType;
use starknet_api::transaction::{Calldata, EventContent, ExecutionResources, MessageToL1};
use starknet_api::{contract_address, patricia_key};
use starknet_types_core::felt::Felt;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::objects::{
    CallType,
    DeclareTransactionTrace,
    DeployAccountTransactionTrace,
    FunctionCall,
    FunctionInvocation,
    FunctionInvocationResult,
    InvokeTransactionTrace,
    L1HandlerTransactionTrace,
    OrderedEvent,
    OrderedL2ToL1Message,
    PriceUnit,
    Retdata,
    RevertReason,
    TransactionTrace,
};
use crate::{BlockExecutionConfig, ExecutionConfigByBlock};

/// Return the default execution config, using the relative path from the testing directory.
pub fn test_get_default_execution_config() -> ExecutionConfigByBlock {
    let execution_config_file = PathBuf::from("../../config/execution/mainnet.json");
    execution_config_file.try_into().unwrap()
}

/// Creates BlockExecutionConfig for tests.
pub fn test_block_execution_config() -> BlockExecutionConfig {
    let execution_config = test_get_default_execution_config();
    let mut block_execution_config =
        execution_config.execution_config_segments.get(&BlockNumber(0)).unwrap().clone();
    block_execution_config.fee_contract_address = contract_address!(0x1001);
    block_execution_config
}

auto_impl_get_test_instance! {
    pub enum TransactionTrace {
        Invoke(InvokeTransactionTrace) = 0,
        Declare(DeclareTransactionTrace) = 1,
        DeployAccount(DeployAccountTransactionTrace) = 2,
    }

    pub struct InvokeTransactionTrace {
        pub validate_invocation: Option<FunctionInvocation>,
        pub execute_invocation: FunctionInvocationResult,
        pub fee_transfer_invocation: Option<FunctionInvocation>,
    }
    pub struct DeclareTransactionTrace {
        pub validate_invocation: Option<FunctionInvocation>,
        pub fee_transfer_invocation: Option<FunctionInvocation>,
    }
    pub struct DeployAccountTransactionTrace {
        pub validate_invocation: Option<FunctionInvocation>,
        pub constructor_invocation: FunctionInvocation,
        pub fee_transfer_invocation: Option<FunctionInvocation>,
    }
    pub struct L1HandlerTransactionTrace {
        pub function_invocation: FunctionInvocation,
    }

    pub enum FunctionInvocationResult {
        Ok(FunctionInvocation) = 0,
        Err(RevertReason) = 1,
    }
    pub enum CallType {
        Call = 0,
        LibraryCall = 1,
    }
    pub struct Retdata(pub Vec<Felt>);
    pub struct OrderedEvent {
        pub order: usize,
        pub event: EventContent,
    }
    pub struct OrderedL2ToL1Message {
        pub order: usize,
        pub message: MessageToL1,
    }
    pub struct FunctionCall {
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub enum PriceUnit {
        Wei = 0,
        Fri = 1,
    }

    pub enum RevertReason {
        RevertReason(String) = 0,
    }
}

impl GetTestInstance for FunctionInvocation {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        Self {
            function_call: FunctionCall::get_test_instance(rng),
            caller_address: ContractAddress::get_test_instance(rng),
            class_hash: ClassHash::get_test_instance(rng),
            entry_point_type: EntryPointType::get_test_instance(rng),
            call_type: CallType::get_test_instance(rng),
            result: Retdata::get_test_instance(rng),
            calls: Vec::new(),
            events: Vec::<OrderedEvent>::get_test_instance(rng),
            messages: Vec::<OrderedL2ToL1Message>::get_test_instance(rng),
            execution_resources: ExecutionResources::get_test_instance(rng),
        }
    }
}
