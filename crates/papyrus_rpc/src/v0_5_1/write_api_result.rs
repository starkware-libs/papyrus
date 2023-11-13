use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::transaction::TransactionHash;
use starknet_client::writer::objects::response::{
    DeclareResponse,
    DeployAccountResponse,
    InvokeResponse,
};

#[cfg(test)]
#[path = "write_api_result_test.rs"]
mod write_api_result_test;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddInvokeOkResult {
    pub transaction_hash: TransactionHash,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddDeclareOkResult {
    pub transaction_hash: TransactionHash,
    pub class_hash: ClassHash,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddDeployAccountOkResult {
    pub transaction_hash: TransactionHash,
    pub contract_address: ContractAddress,
}

impl From<InvokeResponse> for AddInvokeOkResult {
    fn from(response: InvokeResponse) -> Self {
        Self { transaction_hash: response.transaction_hash }
    }
}

impl From<DeclareResponse> for AddDeclareOkResult {
    fn from(response: DeclareResponse) -> Self {
        Self { transaction_hash: response.transaction_hash, class_hash: response.class_hash }
    }
}

impl From<DeployAccountResponse> for AddDeployAccountOkResult {
    fn from(response: DeployAccountResponse) -> Self {
        Self { transaction_hash: response.transaction_hash, contract_address: response.address }
    }
}
