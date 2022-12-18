use std::collections::HashMap;
use std::hash::Hash;

use indexmap::IndexMap;
use rand::Rng;
use starknet_api::block::{
    Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockStatus, BlockTimestamp, GasPrice,
    GlobalRoot,
};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{
    ContractClass, ContractClassAbiEntry, EntryPoint, EntryPointOffset, EntryPointType,
    EventAbiEntry, FunctionAbiEntry, FunctionAbiEntryType, FunctionAbiEntryWithType, Program,
    StateDiff, StorageKey, StructAbiEntry, StructMember, TypedParameter,
};
use starknet_api::transaction::{
    CallData, ContractAddressSalt, DeclareTransaction, DeclareTransactionOutput,
    DeployAccountTransaction, DeployAccountTransactionOutput, DeployTransaction,
    DeployTransactionOutput, EthAddress, Event, EventContent, EventData,
    EventIndexInTransactionOutput, EventKey, Fee, InvokeTransaction, InvokeTransactionOutput,
    L1HandlerTransaction, L1HandlerTransactionOutput, L1ToL2Payload, L2ToL1Payload, MessageToL1,
    MessageToL2, Transaction, TransactionHash, TransactionOffsetInBlock, TransactionOutput,
    TransactionReceipt, TransactionSignature, TransactionVersion,
};
use starknet_api::{patky, shash};

pub trait GetTestInstance: Sized {
    fn get_test_instance() -> Self;
}

auto_impl_get_test_instance! {
    pub struct Block {
        pub header: BlockHeader,
        pub body: BlockBody,
    }
    pub struct BlockHash(pub StarkHash);
    pub struct BlockHeader {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub gas_price: GasPrice,
        pub state_root: GlobalRoot,
        pub sequencer: ContractAddress,
        pub timestamp: BlockTimestamp,
    }
    pub struct BlockNumber(pub u64);
    pub enum BlockStatus {
        Pending = 0,
        AcceptedOnL2 = 1,
        AcceptedOnL1 = 2,
        Rejected = 3,
    }
    pub struct BlockTimestamp(pub u64);
    pub struct CallData(pub Vec<StarkFelt>);
    pub struct ClassHash(pub StarkHash);
    pub struct ContractAddressSalt(pub StarkHash);
    pub struct ContractClass {
        pub abi: Option<Vec<ContractClassAbiEntry>>,
        pub program: Program,
        pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    }
    pub enum ContractClassAbiEntry {
        Event(EventAbiEntry) = 0,
        Function(FunctionAbiEntryWithType) = 1,
        Struct(StructAbiEntry) = 2,
    }
    pub struct DeclareTransaction {
        pub transaction_hash: TransactionHash,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub sender_address: ContractAddress,
    }
    pub struct DeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
    }
    pub struct DeployAccountTransaction {
        pub transaction_hash: TransactionHash,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address: ContractAddress,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: CallData,
    }
    pub struct DeployAccountTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
    }
    pub struct DeployTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub class_hash: ClassHash,
        pub contract_address: ContractAddress,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: CallData,
    }
    pub struct DeployTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
    }
    pub struct EntryPoint {
        pub selector: EntryPointSelector,
        pub offset: EntryPointOffset,
    }
    pub struct EntryPointOffset(pub usize);
    pub struct EntryPointSelector(pub StarkHash);
    pub enum EntryPointType {
        Constructor = 0,
        External = 1,
        L1Handler = 2,
    }
    pub struct Event {
        pub from_address: ContractAddress,
        pub content: EventContent,
    }
    pub struct EventAbiEntry {
        pub name: String,
        pub keys: Vec<TypedParameter>,
        pub data: Vec<TypedParameter>,
    }
    pub struct EventContent {
        pub keys: Vec<EventKey>,
        pub data: EventData,
    }
    pub struct EventData(pub Vec<StarkFelt>);
    pub struct EventIndexInTransactionOutput(pub usize);
    pub struct EventKey(pub StarkFelt);
    pub struct Fee(pub u128);
    pub struct FunctionAbiEntry {
        pub name: String,
        pub inputs: Vec<TypedParameter>,
        pub outputs: Vec<TypedParameter>,
    }
    pub enum FunctionAbiEntryType {
        Constructor = 0,
        L1Handler = 1,
        Regular = 2,
    }
    pub struct FunctionAbiEntryWithType {
        pub r#type: FunctionAbiEntryType,
        pub entry: FunctionAbiEntry,
    }
    pub struct GasPrice(pub u128);
    pub struct GlobalRoot(pub StarkHash);
    pub struct InvokeTransaction {
        pub transaction_hash: TransactionHash,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub contract_address: ContractAddress,
        pub entry_point_selector: Option<EntryPointSelector>,
        pub calldata: CallData,
    }
    pub struct InvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
    }
    pub struct L1HandlerTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub nonce: Nonce,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: CallData,
    }
    pub struct L1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
    }
    pub struct L1ToL2Payload(pub Vec<StarkFelt>);
    pub struct L2ToL1Payload(pub Vec<StarkFelt>);
    pub struct MessageToL1 {
        pub to_address: EthAddress,
        pub payload: L2ToL1Payload,

    }
    pub struct MessageToL2 {
        pub from_address: EthAddress,
        pub payload: L1ToL2Payload,
    }
    pub struct Nonce(pub StarkFelt);
    pub struct Program {
        pub attributes: serde_json::Value,
        pub builtins: serde_json::Value,
        pub compiler_version: serde_json::Value,
        pub data: serde_json::Value,
        pub debug_info: serde_json::Value,
        pub hints: serde_json::Value,
        pub identifiers: serde_json::Value,
        pub main_scope: serde_json::Value,
        pub prime: serde_json::Value,
        pub reference_manager: serde_json::Value,
    }
    pub struct StateDiff {
        pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
        pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
        pub declared_classes: IndexMap<ClassHash, ContractClass>,
        pub nonces: IndexMap<ContractAddress, Nonce>,
    }
    pub struct StructAbiEntry {
        pub name: String,
        pub size: usize,
        pub members: Vec<StructMember>,
    }
    pub struct StructMember {
        pub param: TypedParameter,
        pub offset: usize,
    }
    pub enum Transaction {
        Declare(DeclareTransaction) = 0,
        Deploy(DeployTransaction) = 1,
        DeployAccount(DeployAccountTransaction) = 2,
        Invoke(InvokeTransaction) = 3,
        L1Handler(L1HandlerTransaction) = 4,
    }
    pub struct TransactionHash(pub StarkHash);
    pub struct TransactionOffsetInBlock(pub usize);
    pub enum TransactionOutput {
        Declare(DeclareTransactionOutput) = 0,
        Deploy(DeployTransactionOutput) = 1,
        DeployAccount(DeployAccountTransactionOutput) = 2,
        Invoke(InvokeTransactionOutput) = 3,
        L1Handler(L1HandlerTransactionOutput) = 4,
    }
    pub struct TransactionReceipt {
        pub transaction_hash: TransactionHash,
        pub block_hash: BlockHash,
        pub block_number: BlockNumber,
        pub output: TransactionOutput,
    }
    pub struct TransactionSignature(pub Vec<StarkFelt>);
    pub struct TransactionVersion(pub StarkFelt);
    pub struct TypedParameter {
        pub name: String,
        pub r#type: String,
    }
    bool;
    EthAddress;
    u8;
    u32;
    u64;
    u128;
    usize;
}

#[macro_export]
macro_rules! auto_impl_get_test_instance {
    () => {};
    // Tuple structs (no names associated with fields) - one field.
    ($(pub)? struct $name:ident($(pub)? $ty:ty); $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self(<$ty>::get_test_instance())
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Tuple structs (no names associated with fields) - two fields.
    ($(pub)? struct $name:ident($(pub)? $ty0:ty, $(pub)? $ty1:ty) ; $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self(<$ty0>::get_test_instance(), <$ty1>::get_test_instance())
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Structs with public fields.
    ($(pub)? struct $name:ident { $(pub $field:ident : $ty:ty ,)* } $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self {
                    $(
                        $field: <$ty>::get_test_instance(),
                    )*
                }
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Enums.
    ($(pub)? enum $name:ident { $($variant:ident $( ($ty:ty) )? = $num:expr ,)* } $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                let mut rng = rand::thread_rng();
                let variant = rng.gen_range(0..get_number_of_variants!(enum $name { $($variant $( ($ty) )? = $num ,)* }));
                match variant {
                    $(
                        $num => {
                            Self::$variant$((<$ty>::get_test_instance()))?
                        }
                    )*
                    _ => {
                        panic!("Variant {:?} should match one of the enum variants.", variant);
                    }
                }
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Primitive types.
    ($name:ident; $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self::default()
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    }
}

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for primitive types not
// supported by the macro [`auto_impl_get_test_instance`].
////////////////////////////////////////////////////////////////////////
impl GetTestInstance for serde_json::Value {
    fn get_test_instance() -> Self {
        serde_json::from_str(r#""0x1""#).expect("Failed to convert a 0x1 string to a json value.")
    }
}
impl GetTestInstance for String {
    fn get_test_instance() -> Self {
        "a".to_string()
    }
}
impl<T: GetTestInstance> GetTestInstance for Option<T> {
    fn get_test_instance() -> Self {
        Some(T::get_test_instance())
    }
}
impl<T: GetTestInstance> GetTestInstance for Vec<T> {
    fn get_test_instance() -> Self {
        vec![T::get_test_instance()]
    }
}
impl<K: GetTestInstance + Eq + Hash, V: GetTestInstance> GetTestInstance for HashMap<K, V> {
    fn get_test_instance() -> Self {
        let mut res = HashMap::with_capacity(1);
        let k = K::get_test_instance();
        let v = V::get_test_instance();
        res.insert(k, v);
        res
    }
}
impl<K: GetTestInstance + Eq + Hash, V: GetTestInstance> GetTestInstance for IndexMap<K, V> {
    fn get_test_instance() -> Self {
        let mut res = IndexMap::with_capacity(1);
        let k = K::get_test_instance();
        let v = V::get_test_instance();
        res.insert(k, v);
        res
    }
}
impl<T: GetTestInstance + Default + Copy, const N: usize> GetTestInstance for [T; N] {
    fn get_test_instance() -> Self {
        [T::get_test_instance(); N]
    }
}

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for starknet_api types not
// supported by the macro [`auto_impl_get_test_instance`].
////////////////////////////////////////////////////////////////////////
impl GetTestInstance for StarkHash {
    fn get_test_instance() -> Self {
        shash!("0x1")
    }
}

impl GetTestInstance for ContractAddress {
    fn get_test_instance() -> Self {
        Self(patky!("0x1"))
    }
}

impl GetTestInstance for StorageKey {
    fn get_test_instance() -> Self {
        Self(patky!("0x1"))
    }
}

// Returns a test block body with a single transacion, where the transaction output
// type matches the transaction type.
impl GetTestInstance for BlockBody {
    fn get_test_instance() -> Self {
        let transaction = Transaction::get_test_instance();
        let transaction_output = get_test_transaction_output(&transaction);
        Self { transactions: vec![transaction], transaction_outputs: vec![transaction_output] }
    }
}

fn get_test_transaction_output(transaction: &Transaction) -> TransactionOutput {
    match transaction {
        Transaction::Declare(_) => {
            TransactionOutput::Declare(DeclareTransactionOutput::get_test_instance())
        }
        Transaction::Deploy(_) => {
            TransactionOutput::Deploy(DeployTransactionOutput::get_test_instance())
        }
        Transaction::DeployAccount(_) => {
            TransactionOutput::DeployAccount(DeployAccountTransactionOutput::get_test_instance())
        }
        Transaction::Invoke(_) => {
            TransactionOutput::Invoke(InvokeTransactionOutput::get_test_instance())
        }
        Transaction::L1Handler(_) => {
            TransactionOutput::L1Handler(L1HandlerTransactionOutput::get_test_instance())
        }
    }
}

// Helper macro.
#[macro_export]
macro_rules! get_number_of_variants {
    (enum $name:ident { $($variant:ident $( ($ty:ty) )? = $num:expr ,)* }) => {
        get_number_of_variants!(@count $($variant),+)
    };
    (@count $t1:tt, $($t:tt),+) => { 1 + get_number_of_variants!(@count $($t),+) };
    (@count $t:tt) => { 1 };
}
