use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, Fee, TransactionSignature, TransactionVersion,
};

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
