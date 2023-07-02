use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, Fee, TransactionSignature, TransactionVersion,
};

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeployAccountTransaction {
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    #[serde(default)]
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

// TODO(shahak): Remove code duplication with starknet_reader_client.
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY_ACCOUNT", serialize = "DEPLOY_ACCOUNT"))]
    DeployAccount,
    #[serde(rename(deserialize = "INVOKE_FUNCTION", serialize = "INVOKE_FUNCTION"))]
    #[default]
    InvokeFunction,
}
