use rand::Rng;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;
use starknet_api::transaction::{
    EventIndexInTransactionOutput, Fee, MessageToL1, TransactionOffsetInBlock,
};
use tempfile::tempdir;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput, ThinTransactionOutput,
};
use crate::body::TransactionIndex;
use crate::db::DbConfig;
use crate::state::data::{
    IndexedContractClass, IndexedDeployedContract, IndexedDeprecatedContractClass,
};
use crate::version::Version;
use crate::{
    open_storage, EventIndex, MarkerKind, OmmerEventKey, OmmerTransactionKey, StorageReader,
    StorageWriter,
};

pub fn get_test_config() -> DbConfig {
    let dir = tempdir().unwrap();
    println!("{dir:?}");
    DbConfig {
        path: dir.path().to_path_buf(),
        min_size: 1 << 20,    // 1MB
        max_size: 1 << 35,    // 32GB
        growth_step: 1 << 26, // 64MB
    }
}
pub fn get_test_storage() -> (StorageReader, StorageWriter) {
    let config = get_test_config();
    open_storage(config).unwrap()
}

auto_impl_get_test_instance! {
    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
    pub struct IndexedDeprecatedContractClass {
        pub block_number: BlockNumber,
        pub contract_class: DeprecatedContractClass,
    }
    pub struct IndexedContractClass {
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
    pub enum ThinTransactionOutput {
        Declare(ThinDeclareTransactionOutput) = 0,
        Deploy(ThinDeployTransactionOutput) = 1,
        DeployAccount(ThinDeployAccountTransactionOutput) = 2,
        Invoke(ThinInvokeTransactionOutput) = 3,
        L1Handler(ThinL1HandlerTransactionOutput) = 4,
    }
    struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);
    pub struct Version(pub u32);
}
