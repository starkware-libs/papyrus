use serde::{Deserialize, Serialize};
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::transaction::{Fee, TransactionSignature, TransactionVersion};
use starknet_writer_client::objects::transaction::{
    DeclareV1Transaction, DeployAccountTransaction, InvokeTransaction,
};

pub type BroadcastedDeclareV1Transaction = DeclareV1Transaction;
pub type BroadcastedDeployAccountTransaction = DeployAccountTransaction;
pub type BroadcastedInvokeTransaction = InvokeTransaction;

use crate::v0_3_0::state::ContractClass;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum BroadcastedTransaction {
    #[serde(rename = "DECLARE")]
    // Declare is not from the client because the broadcasted transaction of declare has slight
    // alterations from the client declare.
    Declare(BroadcastedDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(BroadcastedDeployAccountTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(BroadcastedInvokeTransaction),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum BroadcastedDeclareTransaction {
    DeclareV1(BroadcastedDeclareV1Transaction),
    // DeclareV2 is not from the client because the broadcasted transaction of declare v2 has
    // slight alterations from the client declare v2.
    DeclareV2(BroadcastedDeclareV2Transaction),
}

// The only difference between this and DeclareV2Transaction in starknet_writer_client is the
// type of contract_class.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV2Transaction {
    pub contract_class: ContractClass,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
}
