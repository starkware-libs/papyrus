#![allow(clippy::unwrap_used)]
#[cfg(test)]
mod precision_test;

use std::cmp::max;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::read_to_string;
use std::hash::Hash;
use std::net::SocketAddr;
use std::ops::{Deref, Index};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cairo_lang_casm::hints::{CoreHint, CoreHintBase, Hint};
use cairo_lang_casm::operand::{
    BinOpOperand,
    CellRef,
    DerefOrImmediate,
    Operation,
    Register,
    ResOperand,
};
use cairo_lang_starknet::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoint,
    CasmContractEntryPoints,
};
use cairo_lang_utils::bigint::BigUintAsHex;
use indexmap::IndexMap;
use num_bigint::BigUint;
use primitive_types::H160;
use prometheus_parse::Value;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use starknet_api::block::{
    Block,
    BlockBody,
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockSignature,
    BlockStatus,
    BlockTimestamp,
    GasPrice,
};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    GlobalRoot,
    Nonce,
};
use starknet_api::crypto::Signature;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    ContractClassAbiEntry,
    EntryPoint as DeprecatedEntryPoint,
    EntryPointOffset,
    EntryPointType as DeprecatedEntryPointType,
    EventAbiEntry,
    FunctionAbiEntry,
    FunctionStateMutability,
    Program,
    StructAbiEntry,
    StructMember,
    TypedParameter,
};
use starknet_api::state::{
    ContractClass,
    EntryPoint,
    EntryPointType,
    FunctionIndex,
    StateDiff,
    StorageKey,
    ThinStateDiff,
};
use starknet_api::transaction::{
    AccountDeploymentData,
    Builtin,
    Calldata,
    ContractAddressSalt,
    DeclareTransaction,
    DeclareTransactionOutput,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionOutput,
    DeployAccountTransactionV1,
    DeployAccountTransactionV3,
    DeployTransaction,
    DeployTransactionOutput,
    Event,
    EventContent,
    EventData,
    EventIndexInTransactionOutput,
    EventKey,
    ExecutionResources,
    Fee,
    InvokeTransaction,
    InvokeTransactionOutput,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    L1HandlerTransaction,
    L1HandlerTransactionOutput,
    L1ToL2Payload,
    L2ToL1Payload,
    MessageToL1,
    MessageToL2,
    PaymasterData,
    Resource,
    ResourceBounds,
    ResourceBoundsMapping,
    Tip,
    Transaction,
    TransactionExecutionStatus,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionOutput,
    TransactionSignature,
    TransactionVersion,
};
use starknet_types_core::felt::Felt;

//////////////////////////////////////////////////////////////////////////
// GENERIC TEST UTIL FUNCTIONS
//////////////////////////////////////////////////////////////////////////

pub async fn send_request(
    address: SocketAddr,
    method: &str,
    params: &str,
    version: &str,
) -> serde_json::Value {
    let client = Client::new();
    let res_str = client
        .post(format!("http://{address:?}/rpc/{version}"))
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

/// Returns the absolute path from the project root.
pub fn get_absolute_path(relative_path: &str) -> PathBuf {
    Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..").join(relative_path)
}

/// Reads from the directory containing the manifest at run time, same as current working directory.
pub fn read_json_file(path_in_resource_dir: &str) -> serde_json::Value {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap()).unwrap();
    serde_json::from_str(&json_str).unwrap()
}

pub fn validate_load_and_dump<T: Serialize + for<'a> Deserialize<'a>>(path_in_resource_dir: &str) {
    let json_value = read_json_file(path_in_resource_dir);
    let load_result = serde_json::from_value::<T>(json_value.clone());
    assert!(load_result.is_ok(), "error: {:?}", load_result.err());
    let dump_result = serde_json::to_value(&(load_result.unwrap()));
    assert!(dump_result.is_ok(), "error: {:?}", dump_result.err());
    assert_eq!(json_value, dump_result.unwrap());
}

/// Used in random test to create a random generator, see for example storage_serde_test.
/// Randomness can be seeded by setting and env variable `SEED` or by the OS (the rust default).
pub fn get_rng() -> ChaCha8Rng {
    let seed: u64 = match env::var("SEED") {
        Ok(seed_str) => seed_str.parse().unwrap(),
        _ => rand::thread_rng().gen(),
    };
    // Will be printed if the test failed.
    println!("Testing with seed: {seed:?}");
    // Create a new PRNG using a u64 seed. This is a convenience-wrapper around from_seed.
    // It is designed such that low Hamming Weight numbers like 0 and 1 can be used and
    // should still result in good, independent seeds to the returned PRNG.
    // This is not suitable for cryptography purposes.
    ChaCha8Rng::seed_from_u64(seed)
}

/// Use to get the value of a metric by name and labels.
// If the data contains a metric with metric_name and labels returns its value else None.
pub fn prometheus_is_contained(
    data: String,
    metric_name: &str,
    labels: &[(&str, &str)],
) -> Option<Value> {
    // Converts labels to HashMap<String, String>.
    let labels: HashMap<String, String> =
        labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

    let lines: Vec<_> = data.lines().map(|s| Ok(s.to_owned())).collect();
    let metrics = prometheus_parse::Scrape::parse(lines.into_iter()).unwrap();
    for s in metrics.samples {
        if s.metric == metric_name && s.labels.deref() == &labels {
            return Some(s.value);
        }
    }
    None
}

//////////////////////////////////////////////////////////////////////////
// INTERNAL FUNCTIONS
//////////////////////////////////////////////////////////////////////////

/// Returns a test block with a variable number of transactions and events.
fn get_rand_test_block_with_events(
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

// TODO(Dan, 01/11/2023): Remove this util once v3 tests are ready and transaction generation is
// using randomness more stably.
fn is_v3_transaction(transaction: &Transaction) -> bool {
    matches!(
        transaction,
        Transaction::Declare(DeclareTransaction::V3(_))
            | Transaction::DeployAccount(DeployAccountTransaction::V3(_))
            | Transaction::Invoke(InvokeTransaction::V3(_))
    )
}

/// Returns a test block body with a variable number of transactions and events.
fn get_rand_test_body_with_events(
    rng: &mut ChaCha8Rng,
    transaction_count: usize,
    events_per_tx: usize,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> BlockBody {
    let mut transactions = vec![];
    let mut transaction_outputs = vec![];
    let mut transaction_hashes = vec![];
    let mut transaction_execution_statuses = vec![];
    for i in 0..transaction_count {
        let mut transaction = Transaction::get_test_instance(rng);
        while is_v3_transaction(&transaction) {
            transaction = Transaction::get_test_instance(rng);
        }
        transaction_hashes.push(TransactionHash(Felt::from(i)));
        let transaction_output = get_test_transaction_output(&transaction);
        transactions.push(transaction);
        transaction_outputs.push(transaction_output);
        transaction_execution_statuses.push(TransactionExecutionStatus::default());
    }
    let mut body = BlockBody { transactions, transaction_outputs, transaction_hashes };
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

fn get_test_transaction_output(transaction: &Transaction) -> TransactionOutput {
    let mut rng = get_rng();
    let execution_resources = ExecutionResources::get_test_instance(&mut rng);
    match transaction {
        Transaction::Declare(_) => TransactionOutput::Declare(DeclareTransactionOutput {
            execution_resources,
            ..Default::default()
        }),
        Transaction::Deploy(_) => TransactionOutput::Deploy(DeployTransactionOutput {
            execution_resources,
            ..Default::default()
        }),
        Transaction::DeployAccount(_) => {
            TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                execution_resources,
                ..Default::default()
            })
        }
        Transaction::Invoke(_) => TransactionOutput::Invoke(InvokeTransactionOutput {
            execution_resources,
            ..Default::default()
        }),
        Transaction::L1Handler(_) => TransactionOutput::L1Handler(L1HandlerTransactionOutput {
            execution_resources,
            ..Default::default()
        }),
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

//////////////////////////////////////////////////////////////////////////
/// EXTERNAL FUNCTIONS - REMOVE DUPLICATIONS
//////////////////////////////////////////////////////////////////////////

// Returns a test block with a variable number of transactions and events.
pub fn get_test_block(
    transaction_count: usize,
    // TODO(shahak): remove unused event-related arguments.
    events_per_tx: Option<usize>,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> Block {
    let mut rng = get_rng();
    let events_per_tx = if let Some(events_per_tx) = events_per_tx { events_per_tx } else { 0 };
    get_rand_test_block_with_events(
        &mut rng,
        transaction_count,
        events_per_tx,
        from_addresses,
        keys,
    )
}

// Returns a test block body with a variable number of transactions.
pub fn get_test_body(
    transaction_count: usize,
    events_per_tx: Option<usize>,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> BlockBody {
    let mut rng = get_rng();
    let events_per_tx = if let Some(events_per_tx) = events_per_tx { events_per_tx } else { 0 };
    get_rand_test_body_with_events(&mut rng, transaction_count, events_per_tx, from_addresses, keys)
}

// Returns a state diff with one item in each IndexMap.
// For a random test state diff call StateDiff::get_test_instance.
pub fn get_test_state_diff() -> StateDiff {
    let mut rng = get_rng();
    let mut res = StateDiff::get_test_instance(&mut rng);
    // TODO(anatg): fix StateDiff::get_test_instance so the declared_classes will have different
    // hashes than the deprecated_contract_classes.
    let (_, data) = res.declared_classes.pop().unwrap();
    res.declared_classes.insert(ClassHash(Felt::from_hex_unchecked("0x001")), data);
    // TODO(yair): Find a way to create replaced classes in a test instance of StateDiff.
    res.replaced_classes.clear();
    res
}

////////////////////////////////////////////////////////////////////////
// Implementation of GetTestInstance
////////////////////////////////////////////////////////////////////////

pub trait GetTestInstance: Sized {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self;
}

auto_impl_get_test_instance! {
    pub struct AccountDeploymentData(pub Vec<Felt>);
    pub struct BlockHash(pub Felt);
    pub struct BlockHeader {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub eth_l1_gas_price: GasPrice,
        pub strk_l1_gas_price: GasPrice,
        pub state_root: GlobalRoot,
        pub sequencer: ContractAddress,
        pub timestamp: BlockTimestamp,
    }
    pub struct BlockNumber(pub u64);
    pub struct BlockSignature(pub Signature);
    pub enum BlockStatus {
        Pending = 0,
        AcceptedOnL2 = 1,
        AcceptedOnL1 = 2,
        Rejected = 3,
    }
    pub struct BlockTimestamp(pub u64);
    pub enum Builtin {
        RangeCheck = 0,
        Pedersen = 1,
        Poseidon = 2,
        EcOp = 3,
        Ecdsa = 4,
        Bitwise = 5,
        Keccak = 6,
        SegmentArena = 7,
    }
    pub struct Calldata(pub Arc<Vec<Felt>>);
    pub struct ClassHash(pub Felt);
    pub struct CompiledClassHash(pub Felt);
    pub struct ContractAddressSalt(pub Felt);
    pub struct ContractClass {
        pub sierra_program: Vec<Felt>,
        pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
        pub abi: String,
    }
    pub struct DeprecatedContractClass {
        pub abi: Option<Vec<ContractClassAbiEntry>>,
        pub program: Program,
        pub entry_points_by_type: HashMap<DeprecatedEntryPointType, Vec<DeprecatedEntryPoint>>,
    }
    pub enum ContractClassAbiEntry {
        Event(EventAbiEntry) = 0,
        Function(FunctionAbiEntry) = 1,
        Constructor(FunctionAbiEntry) = 2,
        L1Handler(FunctionAbiEntry) = 3,
        Struct(StructAbiEntry) = 4,
    }
    pub enum DataAvailabilityMode {
        L1 = 0,
        L2 = 1,
    }
    pub enum DeclareTransaction {
        V0(DeclareTransactionV0V1) = 0,
        V1(DeclareTransactionV0V1) = 1,
        V2(DeclareTransactionV2) = 2,
        V3(DeclareTransactionV3) = 3,
    }
    pub struct DeclareTransactionV0V1 {
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub sender_address: ContractAddress,
    }
    pub struct DeclareTransactionV2 {
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub compiled_class_hash: CompiledClassHash,
        pub sender_address: ContractAddress,
    }
    pub struct DeclareTransactionV3 {
        pub resource_bounds: ResourceBoundsMapping,
        pub tip: Tip,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub compiled_class_hash: CompiledClassHash,
        pub sender_address: ContractAddress,
        pub nonce_data_availability_mode: DataAvailabilityMode,
        pub fee_data_availability_mode: DataAvailabilityMode,
        pub paymaster_data: PaymasterData,
        pub account_deployment_data: AccountDeploymentData,
    }
    pub enum DeployAccountTransaction {
        V1(DeployAccountTransactionV1) = 0,
        V3(DeployAccountTransactionV3) = 1,
    }
    pub struct DeployAccountTransactionV1 {
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
    }
    pub struct DeployAccountTransactionV3 {
        pub resource_bounds: ResourceBoundsMapping,
        pub tip: Tip,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
        pub nonce_data_availability_mode: DataAvailabilityMode,
        pub fee_data_availability_mode: DataAvailabilityMode,
        pub paymaster_data: PaymasterData,
    }
    pub struct DeployTransaction {
        pub version: TransactionVersion,
        pub class_hash: ClassHash,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
    }
    pub struct DeprecatedEntryPoint {
        pub selector: EntryPointSelector,
        pub offset: EntryPointOffset,
    }
    pub enum DeprecatedEntryPointType {
        Constructor = 0,
        External = 1,
        L1Handler = 2,
    }
    pub struct EntryPoint {
        pub function_idx: FunctionIndex,
        pub selector: EntryPointSelector,
    }
    pub struct Event {
        pub from_address: ContractAddress,
        pub content: EventContent,
    }
    pub struct FunctionIndex(pub usize);
    pub struct EntryPointOffset(pub usize);
    pub struct EntryPointSelector(pub Felt);
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
    pub struct EventData(pub Vec<Felt>);
    pub struct EventIndexInTransactionOutput(pub usize);
    pub struct EventKey(pub Felt);
    pub struct Fee(pub u128);
    pub struct FunctionAbiEntry {
        pub name: String,
        pub inputs: Vec<TypedParameter>,
        pub outputs: Vec<TypedParameter>,
        pub state_mutability: Option<FunctionStateMutability>,
    }
    pub enum FunctionStateMutability {
        View = 0,
    }
    pub struct GasPrice(pub u128);
    pub struct GlobalRoot(pub Felt);
    pub enum InvokeTransaction {
        V0(InvokeTransactionV0) = 0,
        V1(InvokeTransactionV1) = 1,
        V3(InvokeTransactionV3) = 2,
    }
    pub struct InvokeTransactionV0 {
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub struct InvokeTransactionV1 {
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub sender_address: ContractAddress,
        pub calldata: Calldata,
    }
    pub struct InvokeTransactionV3 {
        pub resource_bounds: ResourceBoundsMapping,
        pub tip: Tip,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub sender_address: ContractAddress,
        pub calldata: Calldata,
        pub nonce_data_availability_mode: DataAvailabilityMode,
        pub fee_data_availability_mode: DataAvailabilityMode,
        pub paymaster_data: PaymasterData,
        pub account_deployment_data: AccountDeploymentData,
    }
    pub struct L1HandlerTransaction {
        pub version: TransactionVersion,
        pub nonce: Nonce,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub struct L1ToL2Payload(pub Vec<Felt>);
    pub struct L2ToL1Payload(pub Vec<Felt>);
    pub struct MessageToL1 {
        pub to_address: EthAddress,
        pub payload: L2ToL1Payload,
        pub from_address: ContractAddress,
    }
    pub struct MessageToL2 {
        pub from_address: EthAddress,
        pub payload: L1ToL2Payload,
    }
    pub struct Nonce(pub Felt);
    pub struct PaymasterData(pub Vec<Felt>);
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
    pub enum Resource {
        L1Gas = 0,
        L2Gas = 1,
    }
    pub struct ResourceBounds {
        pub max_amount: u64,
        pub max_price_per_unit: u128,
    }
    pub struct ResourceBoundsMapping(pub BTreeMap<Resource, ResourceBounds>);
        pub struct Signature {
        pub r: Felt,
        pub s: Felt,
    }
    pub struct StateDiff {
        pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
        pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
        pub declared_classes: IndexMap<ClassHash, (CompiledClassHash, ContractClass)>,
        pub deprecated_declared_classes: IndexMap<ClassHash, DeprecatedContractClass>,
        pub nonces: IndexMap<ContractAddress, Nonce>,
        pub replaced_classes: IndexMap<ContractAddress, ClassHash>,
    }
    pub struct StructMember {
        pub param: TypedParameter,
        pub offset: usize,
    }
    pub struct ThinStateDiff {
        pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
        pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, Felt>>,
        pub declared_classes: IndexMap<ClassHash, CompiledClassHash>,
        pub deprecated_declared_classes: Vec<ClassHash>,
        pub nonces: IndexMap<ContractAddress, Nonce>,
        pub replaced_classes: IndexMap<ContractAddress, ClassHash>,
    }
    pub struct Tip(pub u64);
    pub enum Transaction {
        Declare(DeclareTransaction) = 0,
        Deploy(DeployTransaction) = 1,
        DeployAccount(DeployAccountTransaction) = 2,
        Invoke(InvokeTransaction) = 3,
        L1Handler(L1HandlerTransaction) = 4,
    }
    pub enum TransactionExecutionStatus {
        Succeeded = 0,
        Reverted = 1,
    }
    pub struct TransactionHash(pub Felt);
    pub struct TransactionOffsetInBlock(pub usize);
    pub struct TransactionSignature(pub Vec<Felt>);
    pub struct TransactionVersion(pub Felt);
    pub struct TypedParameter {
        pub name: String,
        pub r#type: String,
    }

    pub struct CasmContractClass {
        pub prime: BigUint,
        pub compiler_version: String,
        pub bytecode: Vec<BigUintAsHex>,
        pub hints: Vec<(usize, Vec<Hint>)>,
        pub pythonic_hints: Option<Vec<(usize, Vec<String>)>>,
        pub entry_points_by_type: CasmContractEntryPoints,
    }

    pub struct CasmContractEntryPoints {
        pub external: Vec<CasmContractEntryPoint>,
        pub l1_handler: Vec<CasmContractEntryPoint>,
        pub constructor: Vec<CasmContractEntryPoint>,
    }

    pub struct CasmContractEntryPoint {
        pub selector: BigUint,
        pub offset: usize,
        pub builtins: Vec<String>,
    }

    pub struct BigUintAsHex {
        pub value: BigUint,
    }

    binary(bool);
    binary(EthAddress);
    binary(u8);
    binary(u32);
    binary(u64);
    binary(u128);
    binary(usize);

    (BlockNumber, TransactionOffsetInBlock);
    (BlockHash, ClassHash);
    (ContractAddress, BlockHash);
    (ContractAddress, BlockNumber);
    (ContractAddress, Nonce);
    (ContractAddress, StorageKey, BlockHash);
    (ContractAddress, StorageKey, BlockNumber);
    (CompiledClassHash, ContractClass);
    (usize, Vec<Hint>);
    (usize, Vec<String>);
}

#[macro_export]
macro_rules! auto_impl_get_test_instance {
    () => {};
    // Tuple structs (no names associated with fields) - one field.
    ($(pub)? struct $name:ident($(pub)? $ty:ty); $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
                Self(<$ty>::get_test_instance(rng))
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Tuple structs (no names associated with fields) - two fields.
    ($(pub)? struct $name:ident($(pub)? $ty0:ty, $(pub)? $ty1:ty) ; $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
                Self(<$ty0>::get_test_instance(rng), <$ty1>::get_test_instance(rng))
            }
        }
        auto_impl_get_test_instance!($($rest)*);
    };
    // Structs with public fields.
    ($(pub)? struct $name:ident { $(pub $field:ident : $ty:ty ,)* } $($rest:tt)*) => {
        impl GetTestInstance for $name {
            fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
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
            fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
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
            fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
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
            fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
                use rand::Rng;
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
    (binary($name:ident); $($rest:tt)*) => {
        default_impl_get_test_instance!($name);
        auto_impl_get_test_instance!($($rest)*);
    }
}

#[macro_export]
macro_rules! default_impl_get_test_instance {
    ($name:path) => {
        impl GetTestInstance for $name {
            fn get_test_instance(_rng: &mut rand_chacha::ChaCha8Rng) -> Self {
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
impl<K: GetTestInstance + Eq + Ord, V: GetTestInstance> GetTestInstance for BTreeMap<K, V> {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        let mut res = BTreeMap::new();
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
default_impl_get_test_instance!(H160);
default_impl_get_test_instance!(ContractAddress);
default_impl_get_test_instance!(Felt);
default_impl_get_test_instance!(StorageKey);
default_impl_get_test_instance!(BigUint);

impl GetTestInstance for StructAbiEntry {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self {
            name: String::default(),
            size: 1, // Should be minimum 1.
            members: Vec::<StructMember>::get_test_instance(rng),
        }
    }
}

// Hint Doesn't implement Default - create instance manually.
impl GetTestInstance for Hint {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::Core(CoreHintBase::Core(CoreHint::AllocConstantSize {
            size: ResOperand::BinOp(BinOpOperand {
                op: Operation::Add,
                a: CellRef { register: Register::AP, offset: 0 },
                b: DerefOrImmediate::Deref(CellRef { register: Register::AP, offset: 0 }),
            }),
            dst: CellRef { register: Register::AP, offset: 0 },
        }))
    }
}

impl GetTestInstance for ExecutionResources {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        let rand_not_zero = || max(1, get_rng().next_u64());
        let builtin = Builtin::get_test_instance(rng);
        Self {
            steps: rand_not_zero(),
            builtin_instance_counter: [(builtin, rand_not_zero())].into(),
            memory_holes: rand_not_zero(),
        }
    }
}
