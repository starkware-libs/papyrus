use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    Nonce,
};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    ResourceBoundsMapping,
    Tip,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
};
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::reader::objects::transaction::{
    DeployTransaction,
    IntermediateDeclareTransaction,
    IntermediateDeployAccountTransaction,
    IntermediateInvokeTransaction,
    L1HandlerTransaction,
    Transaction,
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
}
