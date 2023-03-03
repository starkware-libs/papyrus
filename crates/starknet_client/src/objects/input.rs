pub mod transaction{
    use serde::{Serialize, Deserialize};
    use starknet_api::state::ContractClass;
    use starknet_api::transaction::{Fee, TransactionVersion, ContractAddressSalt};
    use starknet_api::core::{Nonce, ClassHash, ContractAddress, EntryPointSelector};
    use crate::objects::transaction::TransactionType;

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
    pub struct StarkFeltAsDecimal(ethnum::U256);

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct TransactionSignature(Vec<StarkFeltAsDecimal>);

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct Calldata(Vec<StarkFeltAsDecimal>);

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
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
    pub struct DeployAccountTransaction{
        #[serde(flatten)]
        pub common_fields: CommonTransactionFields,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
        pub class_hash: ClassHash,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
    pub struct DeployTransaction{
        pub contract_class: ContractClass,
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
        Invoke(InvokeTransaction)
    }

    impl From<starknet_api::hash::StarkFelt> for StarkFeltAsDecimal{
        fn from(felt: starknet_api::hash::StarkFelt) -> Self {
            Self(ethnum::U256::from_be_bytes(felt.bytes().try_into().expect("invalid length")))
        }
    }

    impl From<starknet_api::transaction::Calldata> for Calldata{
        fn from(calldata: starknet_api::transaction::Calldata) -> Self {
            Self(
                calldata.0.iter()
                .map(|d| StarkFeltAsDecimal::from(d.clone()))
                .collect()
            )
        }
    }

    impl From<starknet_api::transaction::TransactionSignature> for TransactionSignature{
        fn from(signature: starknet_api::transaction::TransactionSignature) -> Self {
            Self(
                signature.0.iter()
                .map(|s| StarkFeltAsDecimal::from(s.clone()))
                .collect()
            )
        }
    }

    impl Serialize for StarkFeltAsDecimal{
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer {
            serializer.serialize_str(&self.0.to_string())
        }
    }
}

pub mod block{
    use std::fmt::{self};
    use starknet_api::block::{BlockNumber, BlockHash};

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum Tag {
        /// The most recent fully constructed block
        Latest,
        /// Currently constructed block
        Pending,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum BlockHashOrNumber {
        Hash(BlockHash),
        Number(BlockNumber),
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum BlockId {
        HashOrNumber(BlockHashOrNumber),
        Tag(Tag),
    }

    impl fmt::Display for Tag{
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Tag::Latest => write!(f, "latest"),
                Tag::Pending => write!(f, "pending")
            }
        }
    }

    impl fmt::Display for BlockHashOrNumber{
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                BlockHashOrNumber::Hash(hash) => write!(f, "{}", hash.0),
                BlockHashOrNumber::Number(number) => write!(f, "{}", number.0)
            }
        }
    }

    impl fmt::Display for BlockId{
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self{
                BlockId::Tag(t) => write!(f, "{}", t),
                BlockId::HashOrNumber(h) => write!(f, "{}", h)
            }
        }
    }
}