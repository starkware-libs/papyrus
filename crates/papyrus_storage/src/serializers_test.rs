use std::fmt::Debug;

use indexmap::IndexMap;
use rand::Rng;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StorageKey};
use starknet_api::test_utils::{
    auto_impl_get_test_instance, impl_get_test_instance, GetTestInstance,
};
use starknet_api::transaction::{
    EventIndexInTransactionOutput, Fee, MessageToL1, TransactionOffsetInBlock,
};

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput, ThinTransactionOutput,
};
use crate::db::serialization::StorageSerde;
use crate::state::data::{IndexedDeclaredContract, IndexedDeployedContract, ThinStateDiff};
use crate::{EventIndex, MarkerKind, OmmerEventKey, OmmerTransactionKey, TransactionIndex};

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test();
}

// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() {
        let item = T::get_test_instance();
        let mut serialized: Vec<u8> = Vec::new();
        item.serialize_into(&mut serialized).unwrap();
        let bytes = serialized.into_boxed_slice();
        let deserialized = T::deserialize_from(&mut bytes.as_ref());
        assert_eq!(item, deserialized.unwrap());
    }
}

// Tests all types that implement the [`StorageSerde`] trait
// via the [`auto_storage_serde`] macro.
macro_rules! storage_serde_test {
    ($name:ident) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$name:snake>]() {
                $name::storage_serde_test()
            }
        }
    };
}
pub(crate) use storage_serde_test;

////////////////////////////////////////////////////////////////////////
// Calls the [`storage_serde_test`] macro to create the tests for
// types not supported by the macro [`auto_storage_serde`].
////////////////////////////////////////////////////////////////////////
storage_serde_test!(StarkHash);
storage_serde_test!(ContractAddress);
storage_serde_test!(StorageKey);

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for types defined in this
// crate and create the [`storage_serde_test`] function for them.
////////////////////////////////////////////////////////////////////////
auto_impl_get_test_instance! {
    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
    pub struct IndexedDeclaredContract {
        pub block_number: BlockNumber,
        pub contract_class: ContractClass,
    }
    pub struct IndexedDeployedContract {
        pub block_number: BlockNumber,
        pub class_hash: ClassHash,
    }
    enum MarkerKind {
        Header = 0,
        Body = 1,
        State = 2,
    }
    struct OmmerTransactionKey(pub BlockHash, pub TransactionOffsetInBlock);
    struct OmmerEventKey(pub OmmerTransactionKey, pub EventIndexInTransactionOutput);
    pub struct ThinDeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct ThinDeployTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct ThinDeployAccountTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct ThinInvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct ThinL1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct ThinStateDiff {
        pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
        pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
        pub declared_contract_hashes: Vec<ClassHash>,
        pub nonces: IndexMap<ContractAddress, Nonce>,
    }
    pub enum ThinTransactionOutput {
        Declare(ThinDeclareTransactionOutput) = 0,
        Deploy(ThinDeployTransactionOutput) = 1,
        DeployAccount(ThinDeployAccountTransactionOutput) = 2,
        Invoke(ThinInvokeTransactionOutput) = 3,
        L1Handler(ThinL1HandlerTransactionOutput) = 4,
    }
    struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);
}
