use indexmap::IndexMap;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{ContractClass, StorageKey};
use starknet_api::transaction::{
    EventIndexInTransactionOutput, Fee, MessageToL1, TransactionOffsetInBlock,
};
use tempfile::tempdir;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput, ThinTransactionOutput,
};
use crate::db::DbConfig;
use crate::state::data::{IndexedDeclaredContract, IndexedDeployedContract, ThinStateDiff};
use crate::{
    open_storage, EventIndex, MarkerKind, OmmerEventKey, OmmerTransactionKey, StorageReader,
    StorageWriter, TransactionIndex,
};

pub fn get_test_config() -> DbConfig {
    let dir = tempdir().unwrap();
    DbConfig {
        path: dir.path().to_str().unwrap().to_string(),
        max_size: 1 << 35, // 32GB.
    }
}
pub fn get_test_storage() -> (StorageReader, StorageWriter) {
    let config = get_test_config();
    open_storage(config).unwrap()
}

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
