use std::collections::HashMap;

use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{EntryPoint, EntryPointType};
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Event,
    ExecutionResources,
    Fee,
    L1ToL2Payload,
    L2ToL1Payload,
    PaymasterData,
    ResourceBoundsMapping,
    Tip,
    TransactionExecutionStatus,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionSignature,
    TransactionVersion,
};
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::reader::objects::state::ContractClass;
use crate::reader::objects::transaction::{
    DeployTransaction,
    IntermediateDeclareTransaction,
    IntermediateDeployAccountTransaction,
    IntermediateInvokeTransaction,
    L1HandlerTransaction,
    L1ToL2Message,
    L1ToL2Nonce,
    L2ToL1Message,
    Transaction,
    TransactionReceipt,
};

auto_impl_get_test_instance! {
    pub enum Transaction {
        Declare(IntermediateDeclareTransaction) = 0,
        DeployAccount(IntermediateDeployAccountTransaction) = 1,
        Deploy(DeployTransaction) = 2,
        Invoke(IntermediateInvokeTransaction) = 3,
        L1Handler(L1HandlerTransaction) = 4,
    }
    pub struct IntermediateDeclareTransaction {
        pub resource_bounds: Option<ResourceBoundsMapping>,
        pub tip: Option<Tip>,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub compiled_class_hash: Option<CompiledClassHash>,
        pub sender_address: ContractAddress,
        pub nonce_data_availability_mode: Option<DataAvailabilityMode>,
        pub fee_data_availability_mode: Option<DataAvailabilityMode>,
        pub paymaster_data: Option<PaymasterData>,
        pub account_deployment_data: Option<AccountDeploymentData>,
        pub max_fee: Option<Fee>,
        pub version: TransactionVersion,
        pub transaction_hash: TransactionHash,
    }
    pub struct IntermediateDeployAccountTransaction {
        pub resource_bounds: Option<ResourceBoundsMapping>,
        pub tip: Option<Tip>,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
        pub nonce_data_availability_mode: Option<DataAvailabilityMode>,
        pub fee_data_availability_mode: Option<DataAvailabilityMode>,
        pub paymaster_data: Option<PaymasterData>,
        pub contract_address: ContractAddress,
        pub max_fee: Option<Fee>,
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
    }
    pub struct DeployTransaction {
        pub contract_address: ContractAddress,
        pub contract_address_salt: ContractAddressSalt,
        pub class_hash: ClassHash,
        pub constructor_calldata: Calldata,
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
    }
    pub struct IntermediateInvokeTransaction {
        pub resource_bounds: Option<ResourceBoundsMapping>,
        pub tip: Option<Tip>,
        pub calldata: Calldata,
        pub sender_address: ContractAddress,
        pub entry_point_selector: Option<EntryPointSelector>,
        pub nonce: Option<Nonce>,
        pub max_fee: Option<Fee>,
        pub signature: TransactionSignature,
        pub nonce_data_availability_mode: Option<DataAvailabilityMode>,
        pub fee_data_availability_mode: Option<DataAvailabilityMode>,
        pub paymaster_data: Option<PaymasterData>,
        pub account_deployment_data: Option<AccountDeploymentData>,
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
    }
    pub struct L1HandlerTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub nonce: Nonce,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub struct ContractClass {
        pub sierra_program: Vec<StarkFelt>,
        pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
        pub contract_class_version: String,
        pub abi: String,
    }
    pub struct TransactionReceipt {
        pub transaction_index: TransactionOffsetInBlock,
        pub transaction_hash: TransactionHash,
        pub l1_to_l2_consumed_message: L1ToL2Message,
        pub l2_to_l1_messages: Vec<L2ToL1Message>,
        pub events: Vec<Event>,
        pub execution_resources: ExecutionResources,
        pub actual_fee: Fee,
        pub execution_status: TransactionExecutionStatus,
    }
    pub struct L1ToL2Message {
        pub from_address: EthAddress,
        pub to_address: ContractAddress,
        pub selector: EntryPointSelector,
        pub payload: L1ToL2Payload,
        pub nonce: L1ToL2Nonce,
    }
    pub struct L1ToL2Nonce(pub StarkHash);
    pub struct L2ToL1Message {
        pub from_address: ContractAddress,
        pub to_address: EthAddress,
        pub payload: L2ToL1Payload,
    }
}
