use std::collections::HashMap;
use std::env;
use std::fs::read_to_string;
use std::hash::Hash;
use std::net::SocketAddr;
use std::ops::Index;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use reqwest::Client;
use starknet_api::block::{
    Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockStatus, BlockTimestamp, GasPrice,
};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, GlobalRoot, Nonce};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{
    ContractClass, ContractClassAbiEntry, EntryPoint, EntryPointOffset, EntryPointType,
    EventAbiEntry, FunctionAbiEntry, FunctionAbiEntryType, FunctionAbiEntryWithType, Program,
    StateDiff, StorageKey, StructAbiEntry, StructMember, TypedParameter,
};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, DeclareTransaction, DeclareTransactionOutput,
    DeployAccountTransaction, DeployAccountTransactionOutput, DeployTransaction,
    DeployTransactionOutput, EthAddress, Event, EventContent, EventData,
    EventIndexInTransactionOutput, EventKey, Fee, InvokeTransaction, InvokeTransactionOutput,
    L1HandlerTransaction, L1HandlerTransactionOutput, L1ToL2Payload, L2ToL1Payload, MessageToL1,
    MessageToL2, Transaction, TransactionHash, TransactionOffsetInBlock, TransactionOutput,
    TransactionSignature, TransactionVersion,
};

pub async fn send_request(address: SocketAddr, method: &str, params: &str) -> serde_json::Value {
    let client = Client::new();
    let res_str = client
        .post(format!("http://{address:?}"))
        .header("Content-Type", "application/json")
        .body(format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method}","params":[{params}]}}"#))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&res_str).unwrap()
}

// Returns the absolute  path from the project root.
pub fn get_absolute_path(relative_path: &str) -> PathBuf {
    Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..").join(relative_path)
}

pub fn read_json_file(path_in_resource_dir: &str) -> serde_json::Value {
    // Reads from the directory containing the manifest at run time, same as current working
    // directory.
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap()).unwrap();
    serde_json::from_str(&json_str).unwrap()
}

// Returns a test block with a variable number of transactions and events.
pub fn get_rand_test_block_with_events(
    rng: &mut ChaCha8Rng,
    transaction_count: usize,
    events_per_tx: usize,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> Block {
    Block {
        header: BlockHeader::default(),
        body: get_rand_test_body_with_events(
            rng,
            transaction_count,
            events_per_tx,
            from_addresses,
            keys,
        ),
    }
}

// Returns a test block body with a variable number of transactions and events.
pub fn get_rand_test_body_with_events(
    rng: &mut ChaCha8Rng,
    transaction_count: usize,
    events_per_tx: usize,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> BlockBody {
    let mut transactions = vec![];
    let mut transaction_outputs = vec![];
    for i in 0..transaction_count {
        let mut transaction = Transaction::get_test_instance(rng);
        set_transaction_hash(&mut transaction, TransactionHash(StarkHash::from(i as u64)));
        let transaction_output = get_test_transaction_output(&transaction);
        transactions.push(transaction);
        transaction_outputs.push(transaction_output);
    }
    let mut body = BlockBody { transactions, transaction_outputs };
    for tx_output in &mut body.transaction_outputs {
        let mut events = vec![];
        for _ in 0..events_per_tx {
            let from_address = if let Some(ref options) = from_addresses {
                *options.index(rng.gen_range(0..options.len()))
            } else {
                ContractAddress::default()
            };
            let final_keys = if let Some(ref options) = keys {
                let mut chosen_keys = vec![];
                for options_per_i in options {
                    let key = options_per_i.index(rng.gen_range(0..options_per_i.len())).clone();
                    chosen_keys.push(key);
                }
                chosen_keys
            } else {
                vec![EventKey::default()]
            };
            events.push(Event {
                from_address,
                content: EventContent { keys: final_keys, data: EventData::default() },
            });
        }
        set_events(tx_output, events);
    }
    body
}

// Returns a test block with a variable number of transactions and events.
pub fn get_test_block_with_events(transaction_count: usize, events_per_tx: usize) -> Block {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    get_rand_test_block_with_events(&mut rng, transaction_count, events_per_tx, None, None)
}

// Returns a test block body with a variable number of transactions and events.
pub fn get_test_body_with_events(transaction_count: usize, events_per_tx: usize) -> BlockBody {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    get_rand_test_body_with_events(&mut rng, transaction_count, events_per_tx, None, None)
}

// Returns a test block with a variable number of transactions.
pub fn get_rand_test_block(rng: &mut ChaCha8Rng, transaction_count: usize) -> Block {
    get_rand_test_block_with_events(rng, transaction_count, 0, None, None)
}

// Returns a test block body with a variable number of transactions.
pub fn get_rand_test_body(rng: &mut ChaCha8Rng, transaction_count: usize) -> BlockBody {
    get_rand_test_body_with_events(rng, transaction_count, 0, None, None)
}

// Returns a test block with a variable number of transactions.
pub fn get_test_block(transaction_count: usize) -> Block {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    get_rand_test_block(&mut rng, transaction_count)
}

// Returns a test block body with a variable number of transactions.
pub fn get_test_body(transaction_count: usize) -> BlockBody {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    get_rand_test_body(&mut rng, transaction_count)
}

// Returns a state diff with one item in each IndexMap.
// For a random test state diff call StateDiff::get_test_instance.
pub fn get_test_state_diff() -> StateDiff {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    StateDiff::get_test_instance(&mut rng)
}

// Used in random test to create a random generator, see for example storage_serde_test.
pub fn get_rng() -> ChaCha8Rng {
    let seed = if let Ok(seed_str) = env::var("SEED") {
        seed_str.parse().unwrap()
    } else {
        let mut rng = rand::thread_rng();
        rng.gen()
    };
    // Will be printed if the test failed.
    println!("Testing with seed: {seed:?}");
    // Create a new PRNG using a u64 seed. This is a convenience-wrapper around from_seed.
    // It is designed such that low Hamming Weight numbers like 0 and 1 can be used and
    // should still result in good, independent seeds to the returned PRNG.
    // This is not suitable for cryptography purposes.
    ChaCha8Rng::seed_from_u64(seed)
}

pub trait GetTestInstance: Sized {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self;
}

auto_impl_get_test_instance! {
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
    pub struct Calldata(pub Arc<Vec<StarkFelt>>);
    pub struct ClassHash(pub StarkHash);
    pub struct ContractAddressSalt(pub StarkHash);
    // TODO(anatg): Consider using the compression utils.
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
    pub struct DeployAccountTransaction {
        pub transaction_hash: TransactionHash,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address: ContractAddress,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
    }
    pub struct DeployTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub class_hash: ClassHash,
        pub contract_address: ContractAddress,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
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
        pub sender_address: ContractAddress,
        pub entry_point_selector: Option<EntryPointSelector>,
        pub calldata: Calldata,
    }
    pub struct L1HandlerTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub nonce: Nonce,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub struct L1ToL2Payload(pub Vec<StarkFelt>);
    pub struct L2ToL1Payload(pub Vec<StarkFelt>);
    pub struct MessageToL1 {
        pub from_address: ContractAddress,
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
    pub struct TransactionSignature(pub Vec<StarkFelt>);
    pub struct TransactionVersion(pub StarkFelt);
    pub struct TypedParameter {
        pub name: String,
        pub r#type: String,
    }

    bincode(bool);
    bincode(EthAddress);
    bincode(u8);
    bincode(u32);
    bincode(u64);
    bincode(u128);
    bincode(usize);

    (BlockNumber, TransactionOffsetInBlock);
    (BlockHash, ClassHash);
    (ContractAddress, BlockHash);
    (ContractAddress, BlockNumber);
    (ContractAddress, Nonce);
    (ContractAddress, StorageKey, BlockHash);
    (ContractAddress, StorageKey, BlockNumber);
}

#[macro_export]
macro_rules! auto_impl_get_test_instance {
    () => {};
    // Tuple structs (no names associated with fields) - one field.
    ($(pub)? struct $name:ident($(pub)? $ty:ty); $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
                Self(<$ty>::get_test_instance(rng))
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Tuple structs (no names associated with fields) - two fields.
    ($(pub)? struct $name:ident($(pub)? $ty0:ty, $(pub)? $ty1:ty) ; $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
                Self(<$ty0>::get_test_instance(rng), <$ty1>::get_test_instance(rng))
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Structs with public fields.
    ($(pub)? struct $name:ident { $(pub $field:ident : $ty:ty ,)* } $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
                Self {
                    $(
                        $field: <$ty>::get_test_instance(rng),
                    )*
                }
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Tuples - two elements.
    (($ty0:ty, $ty1:ty) ; $($rest:tt)*) => {
        impl GetTestInstance for ($ty0, $ty1) {
            fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
                (
                    <$ty0>::get_test_instance(rng),
                    <$ty1>::get_test_instance(rng),
                )
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Tuples - three elements.
    (($ty0:ty, $ty1:ty, $ty2:ty) ; $($rest:tt)*) => {
        impl GetTestInstance for ($ty0, $ty1, $ty2) {
            fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
                (
                    <$ty0>::get_test_instance(rng),
                    <$ty1>::get_test_instance(rng),
                    <$ty2>::get_test_instance(rng),
                )
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // enums.
    ($(pub)? enum $name:ident { $($variant:ident $( ($ty:ty) )? = $num:expr ,)* } $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
                let variant = rng.gen_range(0..get_number_of_variants!(enum $name { $($variant $( ($ty) )? = $num ,)* }));
                match variant {
                    $(
                        $num => {
                            Self::$variant$((<$ty>::get_test_instance(rng)))?
                        }
                    )*
                    _ => {
                        panic!("Variant {:?} should match one of the enum {:?} variants.", variant, stringify!($name));
                    }
                }
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Binary.
    (bincode($name:ident); $($rest:tt)*) => {
        default_impl_get_test_instance!($name);
        auto_impl_get_test_instance!($($rest)*);
    }
}

#[macro_export]
macro_rules! default_impl_get_test_instance {
    ($name:path) => {
        impl GetTestInstance for $name {
            fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
                Self::default()
            }
        }
    };
}

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for primitive types.
////////////////////////////////////////////////////////////////////////
default_impl_get_test_instance!(serde_json::Value);
default_impl_get_test_instance!(String);
impl<T: GetTestInstance> GetTestInstance for Arc<T> {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Arc::new(T::get_test_instance(rng))
    }
}
impl<T: GetTestInstance> GetTestInstance for Option<T> {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Some(T::get_test_instance(rng))
    }
}
impl<T: GetTestInstance> GetTestInstance for Vec<T> {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        vec![T::get_test_instance(rng)]
    }
}
impl<K: GetTestInstance + Eq + Hash, V: GetTestInstance> GetTestInstance for HashMap<K, V> {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        let mut res = HashMap::with_capacity(1);
        let k = K::get_test_instance(rng);
        let v = V::get_test_instance(rng);
        res.insert(k, v);
        res
    }
}
impl<K: GetTestInstance + Eq + Hash, V: GetTestInstance> GetTestInstance for IndexMap<K, V> {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        let mut res = IndexMap::with_capacity(1);
        let k = K::get_test_instance(rng);
        let v = V::get_test_instance(rng);
        res.insert(k, v);
        res
    }
}

// Counts the number of variants of an enum.
#[macro_export]
macro_rules! get_number_of_variants {
    (enum $name:ident { $($variant:ident $( ($ty:ty) )? = $num:expr ,)* }) => {
        get_number_of_variants!(@count $($variant),+)
    };
    (@count $t1:tt, $($t:tt),+) => { 1 + get_number_of_variants!(@count $($t),+) };
    (@count $t:tt) => { 1 };
}

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for types not supported
// by the macro [`impl_get_test_instance`].
////////////////////////////////////////////////////////////////////////
default_impl_get_test_instance!(StarkHash);
default_impl_get_test_instance!(ContractAddress);
default_impl_get_test_instance!(StorageKey);

impl GetTestInstance for StructAbiEntry {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self {
            name: String::default(),
            size: 1, // Should be minimum 1.
            members: Vec::<StructMember>::get_test_instance(rng),
        }
    }
}

fn get_test_transaction_output(transaction: &Transaction) -> TransactionOutput {
    match transaction {
        Transaction::Declare(_) => TransactionOutput::Declare(DeclareTransactionOutput::default()),
        Transaction::Deploy(_) => TransactionOutput::Deploy(DeployTransactionOutput::default()),
        Transaction::DeployAccount(_) => {
            TransactionOutput::DeployAccount(DeployAccountTransactionOutput::default())
        }
        Transaction::Invoke(_) => TransactionOutput::Invoke(InvokeTransactionOutput::default()),
        Transaction::L1Handler(_) => {
            TransactionOutput::L1Handler(L1HandlerTransactionOutput::default())
        }
    }
}

fn set_events(tx: &mut TransactionOutput, events: Vec<Event>) {
    match tx {
        TransactionOutput::Declare(tx) => tx.events = events,
        TransactionOutput::Deploy(tx) => tx.events = events,
        TransactionOutput::DeployAccount(tx) => tx.events = events,
        TransactionOutput::Invoke(tx) => tx.events = events,
        TransactionOutput::L1Handler(tx) => tx.events = events,
    }
}

fn set_transaction_hash(tx: &mut Transaction, hash: TransactionHash) {
    match tx {
        Transaction::Declare(tx) => tx.transaction_hash = hash,
        Transaction::Deploy(tx) => tx.transaction_hash = hash,
        Transaction::DeployAccount(tx) => tx.transaction_hash = hash,
        Transaction::Invoke(tx) => tx.transaction_hash = hash,
        Transaction::L1Handler(tx) => tx.transaction_hash = hash,
    }
}
