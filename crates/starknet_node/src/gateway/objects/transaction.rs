use serde::{Deserialize, Serialize};
use starknet_api::{Transaction, TransactionHash};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TypedTransaction>),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "INVOKE", serialize = "INVOKE"))]
    Invoke,
}
impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::Invoke
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TypedTransaction {
    pub r#type: TransactionType,
    #[serde(flatten)]
    pub transaction: Transaction,
}

impl From<Transaction> for TypedTransaction {
    fn from(transaction: Transaction) -> Self {
        match transaction {
            Transaction::Declare(_) => {
                TypedTransaction { r#type: TransactionType::Declare, transaction }
            }
            Transaction::Deploy(_) => {
                TypedTransaction { r#type: TransactionType::Deploy, transaction }
            }
            Transaction::Invoke(_) => {
                TypedTransaction { r#type: TransactionType::Invoke, transaction }
            }
        }
    }
}
