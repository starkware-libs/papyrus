use papyrus_storage::body::events::ThinTransactionOutput;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus};
use starknet_api::core::{ContractAddress, EntryPointSelector, Nonce};
use starknet_api::transaction::{
    Calldata, DeclareTransaction, DeclareTransactionOutput, DeployAccountTransaction,
    DeployAccountTransactionOutput, DeployTransaction, DeployTransactionOutput, Fee,
    InvokeTransactionOutput, L1HandlerTransaction, L1HandlerTransactionOutput, TransactionHash,
    TransactionSignature, TransactionVersion
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TransactionWithType>),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV0 {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl From<starknet_api::transaction::InvokeTransaction> for InvokeTransactionV0 {
    fn from(tx: starknet_api::transaction::InvokeTransaction) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature,
            nonce: tx.nonce,
            contract_address: tx.sender_address,
            entry_point_selector: tx.entry_point_selector.unwrap_or_default(),
            calldata: tx.calldata,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV1 {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
}

impl From<starknet_api::transaction::InvokeTransaction> for InvokeTransactionV1 {
    fn from(tx: starknet_api::transaction::InvokeTransaction) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature,
            nonce: tx.nonce,
            sender_address: tx.sender_address,
            calldata: tx.calldata,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum InvokeTransaction {
    Version0(InvokeTransactionV0),
    Version1(InvokeTransactionV1),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
// Note: When deserializing an untagged enum, no variant can be a prefix of variants to follow.
pub enum Transaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Deploy(DeployTransaction),
    Invoke(InvokeTransaction),
    L1Handler(L1HandlerTransaction),
}

impl Transaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            Transaction::Declare(tx) => tx.transaction_hash,
            Transaction::Deploy(tx) => tx.transaction_hash,
            Transaction::DeployAccount(tx) => tx.transaction_hash,
            Transaction::Invoke(InvokeTransaction::Version0(tx)) => tx.transaction_hash,
            Transaction::Invoke(InvokeTransaction::Version1(tx)) => tx.transaction_hash,
            Transaction::L1Handler(tx) => tx.transaction_hash,
        }
    }
}

impl From<starknet_api::transaction::Transaction> for Transaction {
    fn from(tx: starknet_api::transaction::Transaction) -> Self {
        match tx {
            starknet_api::transaction::Transaction::Declare(declare_tx) => {
                Transaction::Declare(declare_tx)
            }
            starknet_api::transaction::Transaction::Deploy(deploy_tx) => {
                Transaction::Deploy(deploy_tx)
            }
            starknet_api::transaction::Transaction::DeployAccount(deploy_tx) => {
                Transaction::DeployAccount(deploy_tx)
            }
            starknet_api::transaction::Transaction::Invoke(invoke_tx) => {
                if invoke_tx.entry_point_selector.is_none() {
                    Transaction::Invoke(InvokeTransaction::Version1(invoke_tx.into()))
                } else {
                    Transaction::Invoke(InvokeTransaction::Version0(invoke_tx.into()))
                }
            }
            starknet_api::transaction::Transaction::L1Handler(l1_handler_tx) => {
                Transaction::L1Handler(l1_handler_tx)
            }
        }
    }
}

#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "DEPLOY_ACCOUNT", serialize = "DEPLOY_ACCOUNT"))]
    DeployAccount,
    #[serde(rename(deserialize = "INVOKE", serialize = "INVOKE"))]
    #[default]
    Invoke,
    #[serde(rename(deserialize = "L1_HANDLER", serialize = "L1_HANDLER"))]
    L1Handler,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionWithType {
    pub r#type: TransactionType,
    #[serde(flatten)]
    pub transaction: Transaction,
}

impl From<Transaction> for TransactionWithType {
    fn from(transaction: Transaction) -> Self {
        match transaction {
            Transaction::Declare(_) => {
                TransactionWithType { r#type: TransactionType::Declare, transaction }
            }
            Transaction::Deploy(_) => {
                TransactionWithType { r#type: TransactionType::Deploy, transaction }
            }
            Transaction::DeployAccount(_) => {
                TransactionWithType { r#type: TransactionType::DeployAccount, transaction }
            }
            Transaction::Invoke(_) => {
                TransactionWithType { r#type: TransactionType::Invoke, transaction }
            }
            Transaction::L1Handler(_) => {
                TransactionWithType { r#type: TransactionType::L1Handler, transaction }
            }
        }
    }
}

impl From<starknet_api::transaction::Transaction> for TransactionWithType {
    fn from(transaction: starknet_api::transaction::Transaction) -> Self {
        Self::from(Transaction::from(transaction))
    }
}

/// A transaction status in StarkNet.
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionStatus {
    /// The transaction passed the validation and entered the pending block.
    #[serde(rename = "PENDING")]
    Pending,
    /// The transaction passed the validation and entered an actual created block.
    #[serde(rename = "ACCEPTED_ON_L2")]
    #[default]
    AcceptedOnL2,
    /// The transaction was accepted on-chain.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    /// The transaction failed validation.
    #[serde(rename = "REJECTED")]
    Rejected,
}

impl From<BlockStatus> for TransactionStatus {
    fn from(status: BlockStatus) -> Self {
        match status {
            BlockStatus::AcceptedOnL1 => TransactionStatus::AcceptedOnL1,
            BlockStatus::AcceptedOnL2 => TransactionStatus::AcceptedOnL2,
            BlockStatus::Pending => TransactionStatus::Pending,
            BlockStatus::Rejected => TransactionStatus::Rejected,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceiptWithStatus {
    pub status: TransactionStatus,
    #[serde(flatten)]
    pub receipt: TransactionReceipt,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum TransactionReceipt {
    Deploy(DeployTransactionReceipt),
    Common(CommonTransactionReceipt),
}

impl TransactionReceipt {
    pub fn from_transaction_output(
        output: TransactionOutput,
        transaction: &starknet_api::transaction::Transaction,
        block_hash: BlockHash,
        block_number: BlockNumber,
    ) -> Self {
        let common = CommonTransactionReceipt {
            transaction_hash: transaction.transaction_hash(),
            r#type: output.r#type(),
            block_hash,
            block_number,
            output,
        };

        match transaction {
            starknet_api::transaction::Transaction::DeployAccount(tx) => {
                Self::Deploy(DeployTransactionReceipt {
                    common,
                    contract_address: tx.contract_address,
                })
            }
            starknet_api::transaction::Transaction::Deploy(tx) => {
                Self::Deploy(DeployTransactionReceipt {
                    common,
                    contract_address: tx.contract_address,
                })
            }
            _ => Self::Common(common),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransactionReceipt {
    #[serde(flatten)]
    pub common: CommonTransactionReceipt,
    pub contract_address: ContractAddress,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CommonTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub r#type: TransactionType,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum TransactionOutput {
    Declare(DeclareTransactionOutput),
    Deploy(DeployTransactionOutput),
    DeployAccount(DeployAccountTransactionOutput),
    Invoke(InvokeTransactionOutput),
    L1Handler(L1HandlerTransactionOutput),
}

impl TransactionOutput {
    pub fn from_thin_transaction_output(
        thin_tx_output: ThinTransactionOutput,
        events: Vec<starknet_api::transaction::Event>,
    ) -> Self {
        match thin_tx_output {
            ThinTransactionOutput::Declare(thin_declare) => {
                TransactionOutput::Declare(DeclareTransactionOutput {
                    actual_fee: thin_declare.actual_fee,
                    messages_sent: thin_declare.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::Deploy(thin_deploy) => {
                TransactionOutput::Deploy(DeployTransactionOutput {
                    actual_fee: thin_deploy.actual_fee,
                    messages_sent: thin_deploy.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::DeployAccount(thin_deploy) => {
                TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                    actual_fee: thin_deploy.actual_fee,
                    messages_sent: thin_deploy.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::Invoke(thin_invoke) => {
                TransactionOutput::Invoke(InvokeTransactionOutput {
                    actual_fee: thin_invoke.actual_fee,
                    messages_sent: thin_invoke.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::L1Handler(thin_l1handler) => {
                TransactionOutput::L1Handler(L1HandlerTransactionOutput {
                    actual_fee: thin_l1handler.actual_fee,
                    messages_sent: thin_l1handler.messages_sent,
                    events,
                })
            }
        }
    }

    pub fn r#type(&self) -> TransactionType {
        match self {
            TransactionOutput::Declare(_) => TransactionType::Declare,
            TransactionOutput::Deploy(_) => TransactionType::Deploy,
            TransactionOutput::DeployAccount(_) => TransactionType::DeployAccount,
            TransactionOutput::Invoke(_) => TransactionType::Invoke,
            TransactionOutput::L1Handler(_) => TransactionType::L1Handler,
        }
    }
}

impl From<starknet_api::transaction::TransactionOutput> for TransactionOutput {
    fn from(tx_output: starknet_api::transaction::TransactionOutput) -> Self {
        match tx_output {
            starknet_api::transaction::TransactionOutput::Declare(declare_tx_output) => {
                TransactionOutput::Declare(declare_tx_output)
            }
            starknet_api::transaction::TransactionOutput::Deploy(deploy_tx_output) => {
                TransactionOutput::Deploy(deploy_tx_output)
            }
            starknet_api::transaction::TransactionOutput::DeployAccount(deploy_tx_output) => {
                TransactionOutput::DeployAccount(deploy_tx_output)
            }
            starknet_api::transaction::TransactionOutput::Invoke(invoke_tx_output) => {
                TransactionOutput::Invoke(invoke_tx_output)
            }
            starknet_api::transaction::TransactionOutput::L1Handler(l1_handler_tx_output) => {
                TransactionOutput::L1Handler(l1_handler_tx_output)
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub event: starknet_api::transaction::Event,
}


pub mod input{
    use std::collections::HashMap;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde::{Serialize, Deserialize};
    use starknet_api::state::{Program, EntryPointType, EntryPoint};
    use crate::api::{BlockId, Tag};
    use crate::state::{ContractClassAbiEntryWithType};
    use crate::transaction::TransactionType;
    use crate::utils;
    use starknet_api::transaction::{Fee, TransactionVersion, TransactionSignature, ContractAddressSalt, Calldata};
    use starknet_api::core::{Nonce, ClassHash, ContractAddress, EntryPointSelector};

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, PartialOrd, Ord)]
    pub struct CommonTransactionFields{
        pub r#type: TransactionType,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub nonce: Nonce
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct DeclareTransaction{
        #[serde(flatten)]
        pub common_fields: CommonTransactionFields,
        pub contract_class: ContractClass,
        pub sender_address: ContractAddress,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct ContractClass{
        pub abi: Option<Vec<ContractClassAbiEntryWithType>>,
        pub program: Program,
        pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct DeployAccountTransaction{
        #[serde(flatten)]
        pub common_fields: CommonTransactionFields,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
        pub class_hash: ClassHash,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct DeployTransaction{
        pub contract_class: serde_json::Value,
        pub r#type: TransactionType,
        pub version: TransactionVersion,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct InvokeTransactionV0{
        #[serde(flatten)]
        pub common_fields: CommonTransactionFields,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct InvokeTransactionV1{
        #[serde(flatten)]
        pub common_fields: CommonTransactionFields,
        pub sender_address: ContractAddress,
        pub calldata: Calldata
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(untagged)]
    pub enum InvokeTransaction{
        V0(InvokeTransactionV0),
        V1(InvokeTransactionV1)
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    #[serde(untagged)]
    pub enum Transaction{
        Declare(DeclareTransaction),
        DeployAccount(DeployAccountTransaction),
        Deploy(DeployTransaction),
        Invoke(InvokeTransaction),
    }

    impl Into<starknet_client::objects::transaction::TransactionType> for TransactionType{
        fn into(self) -> starknet_client::objects::transaction::TransactionType {
            match self {
                TransactionType::Declare =>         starknet_client::objects::transaction::TransactionType::Declare,
                TransactionType::Deploy =>          starknet_client::objects::transaction::TransactionType::Deploy,
                TransactionType::DeployAccount =>   starknet_client::objects::transaction::TransactionType::DeployAccount,
                TransactionType::Invoke =>          starknet_client::objects::transaction::TransactionType::InvokeFunction,
                TransactionType::L1Handler =>       starknet_client::objects::transaction::TransactionType::L1Handler
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::CommonTransactionFields> for CommonTransactionFields{
        fn into(self) -> starknet_client::objects::input::transaction::CommonTransactionFields {
            starknet_client::objects::input::transaction::CommonTransactionFields {
                r#type: self.r#type.into(),
                max_fee: self.max_fee,
                version: self.version,
                signature: starknet_client::objects::input::transaction::TransactionSignature::from(self.signature),
                nonce: self.nonce
            }
        }
    }

    impl Into<starknet_client::objects::input::block::BlockId> for BlockId{
        fn into(self) -> starknet_client::objects::input::block::BlockId {
            match self {
                BlockId::Tag(t) => {
                    match t{
                        Tag::Latest => starknet_client::objects::input::block::BlockId::Tag(starknet_client::objects::input::block::Tag::Latest),
                        Tag::Pending => starknet_client::objects::input::block::BlockId::Tag(starknet_client::objects::input::block::Tag::Pending)
                    }
                },
                BlockId::HashOrNumber(h) => {
                    match h{
                        crate::api::BlockHashOrNumber::Hash(hash) => starknet_client::objects::input::block::BlockId::HashOrNumber(starknet_client::objects::input::block::BlockHashOrNumber::Hash(hash)),
                        crate::api::BlockHashOrNumber::Number(number) => starknet_client::objects::input::block::BlockId::HashOrNumber(starknet_client::objects::input::block::BlockHashOrNumber::Number(number))
                    }
                }
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::DeployAccountTransaction> for DeployAccountTransaction{
        fn into(self) -> starknet_client::objects::input::transaction::DeployAccountTransaction {
            starknet_client::objects::input::transaction::DeployAccountTransaction{
                common_fields: self.common_fields.into(),
                class_hash: self.class_hash,
                constructor_calldata: starknet_client::objects::input::transaction::Calldata::from(self.constructor_calldata),
                contract_address_salt: self.contract_address_salt
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::DeclareTransaction> for DeclareTransaction{
        fn into(self) -> starknet_client::objects::input::transaction::DeclareTransaction {
            starknet_client::objects::input::transaction::DeclareTransaction{
                common_fields: self.common_fields.into(),
                contract_class: self.contract_class.into(),
                sender_address: self.sender_address
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::InvokeTransactionV0> for InvokeTransactionV0{
        fn into(self) -> starknet_client::objects::input::transaction::InvokeTransactionV0 {
            starknet_client::objects::input::transaction::InvokeTransactionV0{
                common_fields: self.common_fields.into(),
                contract_address: self.contract_address,
                entry_point_selector: self.entry_point_selector,
                calldata: starknet_client::objects::input::transaction::Calldata::from(self.calldata)
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::InvokeTransactionV1> for InvokeTransactionV1{
        fn into(self) -> starknet_client::objects::input::transaction::InvokeTransactionV1 {
            starknet_client::objects::input::transaction::InvokeTransactionV1{
                common_fields: self.common_fields.into(),
                contract_address: self.sender_address,
                calldata: starknet_client::objects::input::transaction::Calldata::from(self.calldata)
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::InvokeTransaction> for InvokeTransaction{
        fn into(self) -> starknet_client::objects::input::transaction::InvokeTransaction {
            match self {
                InvokeTransaction::V0(t) => starknet_client::objects::input::transaction::InvokeTransaction::V0(t.into()),
                InvokeTransaction::V1(t) => starknet_client::objects::input::transaction::InvokeTransaction::V1(t.into()),
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::Transaction> for Transaction{
        fn into(self) -> starknet_client::objects::input::transaction::Transaction {
            match self{
                Transaction::Declare(t) => starknet_client::objects::input::transaction::Transaction::Declare(t.into()),
                Transaction::DeployAccount(t) => starknet_client::objects::input::transaction::Transaction::DeployAccount(t.into()),
                Transaction::Invoke(t) => starknet_client::objects::input::transaction::Transaction::Invoke(t.into()),
                _ => unreachable!("Should not fall in this case")
            }
        }
    }

    impl Into<starknet_client::objects::input::transaction::ContractClass> for ContractClass{
        fn into(self) -> starknet_client::objects::input::transaction::ContractClass {
            let program_value = serde_json::to_value(&self.program).unwrap();
            let program_value = utils::traverse_and_exclude_top_level_keys(
                &program_value, 
                &|key, val|{
                    return (key == "attributes" || key == "compiler_version") && val.is_null();
            });

            let abi = if self.abi.is_none() {
                vec![]
            } else {
                self.abi.unwrap().into_iter().map(|entry| entry.into()).collect()
            };

            let mut en = GzEncoder::new(Vec::new(), Compression::fast());
            serde_json::to_writer(&mut en, &program_value).unwrap();
            let gzip_compressed = en.finish().unwrap();
            let encoded_json = base64::encode(&gzip_compressed);

            starknet_client::objects::input::transaction::ContractClass{
                abi: Some(abi),
                entry_points_by_type: self.entry_points_by_type,
                program: encoded_json
            }
        }
    }
}

pub mod output{
    use serde::{Serialize, Deserialize};


    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub struct FeeEstimate {
        pub gas_consumed: u128,
        pub gas_price: u128,
        pub overall_fee: u128,
    }

    impl From<starknet_client::objects::output::transaction::FeeEstimate> for FeeEstimate{
        fn from(fee: starknet_client::objects::output::transaction::FeeEstimate) -> Self {
            Self { 
                gas_consumed: fee.gas_usage, 
                gas_price: fee.gas_price, 
                overall_fee: fee.overall_fee 
            }
        }
    }
    
}