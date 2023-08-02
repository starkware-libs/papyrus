use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::transaction::TransactionHash;

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
