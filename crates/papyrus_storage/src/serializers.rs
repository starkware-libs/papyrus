#[cfg(test)]
#[path = "serializers_test.rs"]
mod serializers_test;

use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use byteorder::BigEndian;
use cairo_lang_casm::hints::Hint;
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoint,
    CasmContractEntryPoints,
};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_lang_utils::bigint::BigUintAsHex;
use indexmap::IndexMap;
use integer_encoding::*;
use lazy_static::lazy_static;
use num_bigint::BigUint;
use parity_scale_codec::{Decode, Encode};
use primitive_types::H160;
use starknet_api::block::{
    BlockHash,
    BlockNumber,
    BlockSignature,
    BlockStatus,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    EventCommitment,
    GlobalRoot,
    Nonce,
    PatriciaKey,
    SequencerContractAddress,
    TransactionCommitment,
};
use starknet_api::crypto::Signature;
use starknet_api::data_availability::{DataAvailabilityMode, L1DataAvailabilityMode};
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
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{
    ContractClass,
    EntryPoint,
    EntryPointType,
    FunctionIndex,
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
use zstd::bulk::Decompressor;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::body::events::{
    EventIndex,
    ThinDeclareTransactionOutput,
    ThinDeployAccountTransactionOutput,
    ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput,
    ThinL1HandlerTransactionOutput,
    ThinTransactionOutput,
};
use crate::body::TransactionIndex;
use crate::compression_utils::{
    compress,
    decompress,
    decompress_from_reader,
    serialize_and_compress,
    IsCompressed,
};
use crate::db::serialization::{
    StorageSerde,
    StorageSerdeError,
    ValueSerde,
    VersionOneWrapper,
    VERSION_ONE,
};
use crate::db::DbError;
use crate::header::StorageBlockHeader;
use crate::mmap_file::LocationInFile;
#[cfg(test)]
use crate::serializers::serializers_test::{create_storage_serde_test, StorageSerdeTest};
use crate::state::data::IndexedDeprecatedContractClass;
use crate::version::Version;
use crate::{MarkerKind, OffsetKind, TransactionMetadata};

// The threshold for compressing transactions.
const COMPRESSION_THRESHOLD_BYTES: usize = 384;

auto_storage_serde! {
    pub struct AccountDeploymentData(pub Vec<StarkFelt>);
    pub struct BlockHash(pub StarkHash);
    pub struct StorageBlockHeader {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub l1_gas_price: GasPricePerToken,
        pub l1_data_gas_price: GasPricePerToken,
        pub state_root: GlobalRoot,
        pub sequencer: SequencerContractAddress,
        pub timestamp: BlockTimestamp,
        pub l1_da_mode: L1DataAvailabilityMode,
        pub transaction_commitment: TransactionCommitment,
        pub event_commitment: EventCommitment,
        pub n_transactions: usize,
        pub n_events: usize,
    }
    pub struct BlockSignature(pub Signature);
    pub enum BlockStatus {
        Pending = 0,
        AcceptedOnL2 = 1,
        AcceptedOnL1 = 2,
        Rejected = 3,
    }
    pub struct BlockTimestamp(pub u64);
    pub struct Calldata(pub Arc<Vec<StarkFelt>>);
    pub struct CompiledClassHash(pub StarkHash);
    pub struct ClassHash(pub StarkHash);
    pub struct ContractAddressSalt(pub StarkHash);
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
    pub struct DeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
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
    pub struct DeployAccountTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct DeployTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
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
    pub struct FunctionIndex(pub usize);
    pub struct EntryPointOffset(pub usize);
    pub struct EntryPointSelector(pub StarkHash);
    pub enum EntryPointType {
        Constructor = 0,
        External = 1,
        L1Handler = 2,
    }
    // TODO(dan): consider implementing directly with no H160 dependency.
    pub struct EthAddress(pub H160);
    pub struct EventAbiEntry {
        pub name: String,
        pub keys: Vec<TypedParameter>,
        pub data: Vec<TypedParameter>,
    }
    pub struct Event {
        pub from_address: ContractAddress,
        pub content: EventContent,
    }
    pub struct EventContent {
        pub keys: Vec<EventKey>,
        pub data: EventData,
    }
    pub struct EventCommitment(pub StarkHash);
    pub struct EventData(pub Vec<StarkFelt>);
    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
    pub struct EventIndexInTransactionOutput(pub usize);
    pub struct EventKey(pub StarkFelt);
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
    pub struct GasPricePerToken {
        pub price_in_fri: GasPrice,
        pub price_in_wei: GasPrice,
    }
    pub struct GlobalRoot(pub StarkHash);
    pub struct H160(pub [u8; 20]);
    pub struct IndexedDeprecatedContractClass {
        pub block_number: BlockNumber,
        pub location_in_file: LocationInFile,
    }
    pub enum InvokeTransaction {
        V0(InvokeTransactionV0) = 0,
        V1(InvokeTransactionV1) = 1,
        V3(InvokeTransactionV3) = 2,
    }
    pub struct InvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub enum IsCompressed {
        No = 0,
        Yes = 1,
    }
    pub enum L1DataAvailabilityMode {
        Calldata = 0,
        Blob = 1,
    }
    pub struct L1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events: Vec<Event>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct L1ToL2Payload(pub Vec<StarkFelt>);
    pub struct L2ToL1Payload(pub Vec<StarkFelt>);
    enum MarkerKind {
        Header = 0,
        Body = 1,
        State = 2,
        CompiledClass = 3,
        BaseLayerBlock = 4,
    }
    pub struct MessageToL1 {
        pub to_address: EthAddress,
        pub payload: L2ToL1Payload,
        pub from_address: ContractAddress,
    }
    pub struct MessageToL2 {
        pub from_address: EthAddress,
        pub payload: L1ToL2Payload,
    }
    pub enum NestedIntList {
        Leaf(usize) = 0,
        Node(Vec<NestedIntList>) = 1,
    }
    pub struct Nonce(pub StarkFelt);
    pub enum OffsetKind {
        ThinStateDiff = 0,
        ContractClass = 1,
        Casm = 2,
        DeprecatedContractClass = 3,
        TransactionOutput = 4,
        Transaction = 5,
    }
    pub struct PaymasterData(pub Vec<StarkFelt>);
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
    pub struct SequencerContractAddress(pub ContractAddress);
    pub struct Signature {
        pub r: StarkFelt,
        pub s: StarkFelt,
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
    pub struct StarknetVersion(pub String);
    pub struct Tip(pub u64);
    pub struct ThinDeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinDeployTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinDeployAccountTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct TransactionCommitment(pub StarkHash);
    pub struct TypedParameter {
        pub name: String,
        pub r#type: String,
    }
    // TODO(dvir): change the macro to get also pub(crate) and private visibility and change this struct
    // visibility in the macro.
    pub struct TransactionMetadata {
        pub tx_hash: TransactionHash,
        pub tx_location: LocationInFile,
        pub tx_output_location: LocationInFile,
    }
    pub struct ThinInvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinL1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub enum ThinTransactionOutput {
        Declare(ThinDeclareTransactionOutput) = 0,
        Deploy(ThinDeployTransactionOutput) = 1,
        DeployAccount(ThinDeployAccountTransactionOutput) = 2,
        Invoke(ThinInvokeTransactionOutput) = 3,
        L1Handler(ThinL1HandlerTransactionOutput) = 4,
    }
    pub enum Transaction {
        Declare(DeclareTransaction) = 0,
        Deploy(DeployTransaction) = 1,
        DeployAccount(DeployAccountTransaction) = 2,
        Invoke(InvokeTransaction) = 3,
        L1Handler(L1HandlerTransaction) = 4,
    }
    pub struct ThinStateDiff {
        pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
        pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
        pub declared_classes: IndexMap<ClassHash, CompiledClassHash>,
        pub deprecated_declared_classes: Vec<ClassHash>,
        pub nonces: IndexMap<ContractAddress, Nonce>,
        pub replaced_classes: IndexMap<ContractAddress, ClassHash>,
    }
    pub enum TransactionExecutionStatus {
        Succeeded = 0,
        Reverted = 1,
    }
    pub struct TransactionHash(pub StarkHash);
    struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);
    pub enum TransactionOutput {
        Declare(DeclareTransactionOutput) = 0,
        Deploy(DeployTransactionOutput) = 1,
        DeployAccount(DeployAccountTransactionOutput) = 2,
        Invoke(InvokeTransactionOutput) = 3,
        L1Handler(L1HandlerTransactionOutput) = 4,
    }
    pub struct TransactionSignature(pub Vec<StarkFelt>);
    pub struct TransactionVersion(pub StarkFelt);
    pub struct Version(pub u32);

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

    pub struct ExecutionResources {
        pub steps: u64,
        pub builtin_instance_counter: HashMap<Builtin, u64>,
        pub memory_holes: u64,
    }
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

    binary(u32, read_u32, write_u32);
    binary(u64, read_u64, write_u64);
    binary(u128, read_u128, write_u128);


    (BlockNumber, TransactionOffsetInBlock);
    (BlockHash, ClassHash);
    (ContractAddress, BlockHash);
    (ContractAddress, BlockNumber);
    (ContractAddress, Nonce);
    (ContractAddress, EventIndex);
    (ContractAddress, StorageKey, BlockHash);
    (ContractAddress, StorageKey, BlockNumber);
    (usize, Vec<Hint>);
    (usize, Vec<String>);
}

////////////////////////////////////////////////////////////////////////
//  impl StorageSerde macro.
////////////////////////////////////////////////////////////////////////
macro_rules! auto_storage_serde {
    () => {};
    // Tuple structs (no names associated with fields) - one field.
    ($(pub)? struct $name:ident($(pub)? $ty:ty); $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                self.0.serialize_into(res)
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some(Self (<$ty>::deserialize_from(bytes)?))
            }
        }
        #[cfg(test)]
        create_storage_serde_test!($name);
        auto_storage_serde!($($rest)*);
    };
    // Tuple structs (no names associated with fields) - two fields.
    ($(pub)? struct $name:ident($(pub)? $ty0:ty, $(pub)? $ty1:ty) ; $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                self.0.serialize_into(res)?;
                self.1.serialize_into(res)
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some($name(<$ty0>::deserialize_from(bytes)?, <$ty1>::deserialize_from(bytes)?))
            }
        }
        #[cfg(test)]
        create_storage_serde_test!($name);
        auto_storage_serde!($($rest)*);
    };
    // Structs with public fields.
    ($(pub)? struct $name:ident { $(pub $field:ident : $ty:ty ,)* } $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                $(
                    self.$field.serialize_into(res)?;
                )*
                Ok(())
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some(Self {
                    $(
                        $field: <$ty>::deserialize_from(bytes)?,
                    )*
                })
            }
        }
        #[cfg(test)]
        create_storage_serde_test!($name);
        auto_storage_serde!($($rest)*);
    };
    // Tuples - two elements.
    (($ty0:ty, $ty1:ty) ; $($rest:tt)*) => {
        impl StorageSerde for ($ty0, $ty1) {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                self.0.serialize_into(res)?;
                self.1.serialize_into(res)
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some((
                    <$ty0>::deserialize_from(bytes)?,
                    <$ty1>::deserialize_from(bytes)?,
                ))
            }
        }
        auto_storage_serde!($($rest)*);
    };
    // Tuples - three elements.
    (($ty0:ty, $ty1:ty, $ty2:ty) ; $($rest:tt)*) => {
        impl StorageSerde for ($ty0, $ty1, $ty2) {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                self.0.serialize_into(res)?;
                self.1.serialize_into(res)?;
                self.2.serialize_into(res)
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some((
                    <$ty0>::deserialize_from(bytes)?,
                    <$ty1>::deserialize_from(bytes)?,
                    <$ty2>::deserialize_from(bytes)?,
                ))
            }
        }
        auto_storage_serde!($($rest)*);
    };
    // enums.
    ($(pub)? enum $name:ident { $($variant:ident $( ($ty:ty) )? = $num:expr ,)* } $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                match self {
                    $(
                        variant!( value, $variant $( ($ty) )?) => {
                            res.write_all(&[$num as u8])?;
                            $(
                                (value as &$ty).serialize_into(res)?;
                            )?
                            Ok(())
                        }
                    )*
                }
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                let mut kind = [0u8; 1];
                bytes.read_exact(&mut kind).ok()?;
                match kind[0] {
                    $(
                        $num => {
                            Some(Self::$variant $( (<$ty>::deserialize_from(bytes)?) )? )
                        },
                    )*
                    _ => None,}
            }
        }
        #[cfg(test)]
        create_storage_serde_test!($name);
        auto_storage_serde!($($rest)*);
    };
    // Binary.
    (binary($name:ident, $read:ident, $write:ident); $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                Ok(byteorder::WriteBytesExt::$write::<BigEndian>(res, *self)?)
            }

            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                byteorder::ReadBytesExt::$read::<BigEndian>(bytes).ok()
            }
        }
        #[cfg(test)]
        create_storage_serde_test!($name);
        auto_storage_serde!($($rest)*);
    }
}
pub(crate) use auto_storage_serde;

// Helper macro.
macro_rules! variant {
    ($value:ident, $variant:ident) => {
        Self::$variant
    };
    ($value:ident, $variant:ident($ty:ty)) => {
        Self::$variant($value)
    };
}
pub(crate) use variant;

////////////////////////////////////////////////////////////////////////
// Starknet API structs.
////////////////////////////////////////////////////////////////////////
impl StorageSerde for ContractAddress {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.0.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        ContractAddress::try_from(StarkHash::deserialize(bytes)?).ok()
    }
}

impl StorageSerde for PatriciaKey {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.key().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Self::try_from(StarkHash::deserialize(bytes)?).ok()
    }
}

impl StorageSerde for StarkHash {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        Ok(self.serialize(res)?)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Self::deserialize(bytes)
    }
}

impl StorageSerde for StorageKey {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.0.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        StorageKey::try_from(StarkHash::deserialize(bytes)?).ok()
    }
}

////////////////////////////////////////////////////////////////////////
//  Primitive types.
////////////////////////////////////////////////////////////////////////
impl StorageSerde for bool {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        u8::from(*self).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some((u8::deserialize_from(bytes)?) != 0)
    }
}

// TODO(spapini): Perhaps compress this textual data.
impl StorageSerde for serde_json::Value {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let bytes = serde_json::to_vec(self)?;
        bytes.serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let buf = Vec::deserialize_from(bytes)?;
        serde_json::from_slice(buf.as_slice()).ok()
    }
}

impl StorageSerde for String {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        (self.as_bytes().to_vec()).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Self::from_utf8(Vec::deserialize_from(bytes)?).ok()
    }
}

impl<T: StorageSerde> StorageSerde for Option<T> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        match self {
            Some(value) => {
                res.write_all(&[1])?;
                value.serialize_into(res)
            }
            None => Ok(res.write_all(&[0])?),
        }
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let mut exists = [0u8; 1];
        bytes.read_exact(&mut exists).ok()?;
        match exists[0] {
            0 => Some(None),
            1 => Some(Some(T::deserialize_from(bytes)?)),
            _ => None,
        }
    }
}

impl StorageSerde for u8 {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        Ok(byteorder::WriteBytesExt::write_u8(res, *self)?)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        byteorder::ReadBytesExt::read_u8(bytes).ok()
    }
}

// TODO(dan): get rid of usize.
impl StorageSerde for usize {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        (*self as u64).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        usize::try_from(u64::deserialize_from(bytes)?).ok()
    }
}

impl<T: StorageSerde> StorageSerde for Vec<T> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        res.write_varint(self.len())?;
        for x in self {
            x.serialize_into(res)?
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let n: usize = bytes.read_varint().ok()?;
        let mut res = Vec::with_capacity(n);
        for _i in 0..n {
            res.push(T::deserialize_from(bytes)?);
        }
        Some(res)
    }
}
impl<K: StorageSerde + Eq + Hash, V: StorageSerde> StorageSerde for HashMap<K, V> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        res.write_varint(self.len())?;
        for (k, v) in self.iter() {
            k.serialize_into(res)?;
            v.serialize_into(res)?;
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let n: usize = bytes.read_varint().ok()?;
        let mut res = HashMap::with_capacity(n);
        for _i in 0..n {
            let k = K::deserialize_from(bytes)?;
            let v = V::deserialize_from(bytes)?;
            if res.insert(k, v).is_some() {
                return None;
            }
        }
        Some(res)
    }
}
// TODO(anatg): Find a way to share code with StorageSerde for HashMap.
impl<K: StorageSerde + Eq + Hash, V: StorageSerde> StorageSerde for IndexMap<K, V> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        res.write_varint(self.len())?;
        for (k, v) in self.iter() {
            k.serialize_into(res)?;
            v.serialize_into(res)?;
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let n: usize = bytes.read_varint().ok()?;
        let mut res = IndexMap::with_capacity(n);
        for _i in 0..n {
            let k = K::deserialize_from(bytes)?;
            let v = V::deserialize_from(bytes)?;
            if res.insert(k, v).is_some() {
                return None;
            }
        }
        Some(res)
    }
}
impl<K: StorageSerde + Eq + Ord, V: StorageSerde> StorageSerde for BTreeMap<K, V> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        res.write_varint(self.len())?;
        for (k, v) in self.iter() {
            k.serialize_into(res)?;
            v.serialize_into(res)?;
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let n: usize = bytes.read_varint().ok()?;
        let mut res = BTreeMap::new();
        for _i in 0..n {
            let k = K::deserialize_from(bytes)?;
            let v = V::deserialize_from(bytes)?;
            if res.insert(k, v).is_some() {
                return None;
            }
        }
        Some(res)
    }
}
impl<T: StorageSerde + Default + Copy, const N: usize> StorageSerde for [T; N] {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        for x in self {
            x.serialize_into(res)?;
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let mut res = [T::default(); N];
        for elm in res.iter_mut() {
            *elm = T::deserialize_from(bytes)?;
        }
        Some(res)
    }
}
impl<T: StorageSerde> StorageSerde for Arc<T> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.deref().serialize_into(res)?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let res = T::deserialize_from(bytes)?;
        Some(Arc::new(res))
    }
}

impl StorageSerde for Hint {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.encode().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Self::decode(&mut Vec::<u8>::deserialize_from(bytes)?.as_slice()).ok()
    }
}

impl StorageSerde for BigUint {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.to_bytes_be().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let bytes_be = Vec::<u8>::deserialize_from(bytes)?;
        Some(BigUint::from_bytes_be(bytes_be.as_slice()))
    }
}

////////////////////////////////////////////////////////////////////////
//  Custom serialization for storage reduction.
////////////////////////////////////////////////////////////////////////
// TODO(dvir): remove this when BlockNumber will be u32.
impl StorageSerde for BlockNumber {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        (self.0 as u32).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(BlockNumber(u32::deserialize_from(bytes)? as u64))
    }
}

// This serialization write the offset as 3 bytes, which means that the maximum offset is ~16
// million (1<<24 bytes).
impl StorageSerde for TransactionOffsetInBlock {
    fn serialize_into(
        &self,
        res: &mut impl std::io::Write,
    ) -> Result<(), crate::db::serialization::StorageSerdeError> {
        let bytes = &self.0.to_le_bytes();
        res.write_all(&bytes[0..3])?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let mut arr = [0u8; 8];
        bytes.read_exact(&mut arr[0..3]).ok()?;
        let index = usize::from_le_bytes(arr);
        Some(Self(index))
    }
}

// A value place holder for tables where we don't need a value.
#[derive(Debug, Clone)]
pub(crate) struct ValuePlaceHolder;

impl StorageSerde for ValuePlaceHolder {
    fn serialize_into(&self, _res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        Ok(())
    }

    fn deserialize_from(_bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(Self)
    }
}

////////////////////////////////////////////////////////////////////////
//  Custom serialization with pre trained dictionaries compression.
////////////////////////////////////////////////////////////////////////

// TODO(dvir): fine tune the compression hyperparameters, especially compression level, magic_bytes
// and back reference distance.

// ThinStateDiff compression.
const THIN_STATE_DIFF_COMPRESSION_DICT_V1: &[u8] = &[0];
lazy_static! {
    static ref THIN_STATE_DIFF_ENCODER_DICT: EncoderDictionary<'static> = EncoderDictionary::new(
        THIN_STATE_DIFF_COMPRESSION_DICT_V1,
        zstd::DEFAULT_COMPRESSION_LEVEL
    );
    static ref THIN_STATE_DIFF_DECODER_DICT_V1: DecoderDictionary<'static> =
        DecoderDictionary::new(THIN_STATE_DIFF_COMPRESSION_DICT_V1);
    static ref THIN_STATE_DIFF_DECODERS_ARRAY: [&'static DecoderDictionary<'static>; 1] =
        [&THIN_STATE_DIFF_DECODER_DICT_V1];
}

impl ValueSerde for VersionOneWrapper<ThinStateDiff> {
    type Value = ThinStateDiff;

    fn serialize(obj: &Self::Value) -> Result<Vec<u8>, DbError> {
        serialize_with_compression(obj, VERSION_ONE, &THIN_STATE_DIFF_ENCODER_DICT)
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self::Value> {
        deserialize_compressed(bytes, &*THIN_STATE_DIFF_DECODERS_ARRAY)
    }
}

// An upper bound for the size of the decompressed data.
const MAX_DECOMPRESSION_CAPACITY: usize = 1 << 32; // 4GB

fn serialize_with_compression<T: StorageSerde>(
    obj: &T,
    version: u8,
    encoder_dict: &EncoderDictionary<'static>,
) -> Result<Vec<u8>, crate::db::DbError> {
    let mut buff = Vec::new();
    obj.serialize_into(&mut buff).map_err(|_| DbError::Serialization)?;
    let compressed_buff_size = zstd::zstd_safe::compress_bound(buff.len());
    // TODO(dvir): remove this when using the same buffer for all the serialization in write
    // operation.
    let mut res = vec![0; compressed_buff_size + 1];

    // TODO(dvir): create one compressor for all the serialization in write operation.
    let mut compressor = zstd::bulk::Compressor::with_prepared_dictionary(encoder_dict)
        .map_err(|_| DbError::Compression)?;
    res[0] = version;
    let compressed_size = compressor
        .compress_to_buffer(buff.as_slice(), &mut res[1..])
        .map_err(|_| DbError::Compression)?;

    // Truncate the buffer to get rid of the zeroes at the end.
    res.truncate(compressed_size + 1);
    Ok(res)
}

fn deserialize_compressed<T: StorageSerde>(
    bytes: &mut impl std::io::Read,
    dicts: &[&'static DecoderDictionary<'static>],
) -> Option<T> {
    let mut version = [0u8; 1];
    bytes.read_exact(&mut version[..]).ok()?;

    // TODO(dvir): try to create decompressor once for each thread.
    if version[0] as usize > dicts.len() {
        return None;
    }
    let decoder_dict = dicts[version[0] as usize - 1];
    let mut decompressor = Decompressor::with_prepared_dictionary(decoder_dict).ok()?;

    // TODO(dvir): consider change the ValueSerde trait to get &[u8] when deserializing and thus
    // avoid to read the whole buffer first (this is copy all the buffer).
    let mut to_decompress = Vec::new();
    if bytes.read_to_end(&mut to_decompress).is_err() {
        return None;
    }
    // TODO(dvir): try to create one buffer for each thread and decompress to it instead of
    // creating new buffer each time.
    let decompressed_bytes =
        decompressor.decompress(&to_decompress, MAX_DECOMPRESSION_CAPACITY).ok()?;
    let mut decompressed_bytes = decompressed_bytes.as_slice();
    let res = T::deserialize_from(&mut decompressed_bytes);

    // Make sure we are at EOF.
    if !decompressed_bytes.is_empty() {
        return None;
    }

    res
}

////////////////////////////////////////////////////////////////////////
//  Custom serialization with compression.
////////////////////////////////////////////////////////////////////////

impl StorageSerde for ContractClass {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        serialize_and_compress(&self.sierra_program)?.serialize_into(res)?;
        self.entry_points_by_type.serialize_into(res)?;
        serialize_and_compress(&self.abi)?.serialize_into(res)?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(Self {
            sierra_program: Vec::<StarkFelt>::deserialize_from(
                &mut decompress_from_reader(bytes)?.as_slice(),
            )?,
            entry_points_by_type: HashMap::<EntryPointType, Vec<EntryPoint>>::deserialize_from(
                bytes,
            )?,
            abi: String::deserialize_from(&mut decompress_from_reader(bytes)?.as_slice())?,
        })
    }
}
#[cfg(test)]
create_storage_serde_test!(ContractClass);

impl StorageSerde for DeprecatedContractClass {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        // Compress together the program and abi for better compression results.
        let mut to_compress: Vec<u8> = Vec::new();
        self.abi.serialize_into(&mut to_compress)?;
        self.program.serialize_into(&mut to_compress)?;
        let compressed = compress(to_compress.as_slice())?;
        compressed.serialize_into(res)?;
        self.entry_points_by_type.serialize_into(res)?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let compressed_data = Vec::<u8>::deserialize_from(bytes)?;
        let data = decompress(compressed_data.as_slice()).ok()?;
        let data = &mut data.as_slice();
        Some(Self {
            abi: Option::<Vec<ContractClassAbiEntry>>::deserialize_from(data)?,
            program: Program::deserialize_from(data)?,
            entry_points_by_type:
                HashMap::<DeprecatedEntryPointType, Vec<DeprecatedEntryPoint>>::deserialize_from(
                    bytes,
                )?,
        })
    }
}
#[cfg(test)]
create_storage_serde_test!(DeprecatedContractClass);

impl StorageSerde for CasmContractClass {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let mut to_compress: Vec<u8> = Vec::new();
        self.prime.serialize_into(&mut to_compress)?;
        self.compiler_version.serialize_into(&mut to_compress)?;
        self.bytecode.serialize_into(&mut to_compress)?;
        self.bytecode_segment_lengths.serialize_into(&mut to_compress)?;
        self.hints.serialize_into(&mut to_compress)?;
        self.pythonic_hints.serialize_into(&mut to_compress)?;
        self.entry_points_by_type.serialize_into(&mut to_compress)?;

        let compressed = compress(to_compress.as_slice())?;
        compressed.serialize_into(res)?;

        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let compressed_data = Vec::<u8>::deserialize_from(bytes)?;
        let data = decompress(compressed_data.as_slice()).ok()?;
        let data = &mut data.as_slice();
        Some(Self {
            prime: BigUint::deserialize_from(data)?,
            compiler_version: String::deserialize_from(data)?,
            bytecode: Vec::<BigUintAsHex>::deserialize_from(data)?,
            bytecode_segment_lengths: Option::<NestedIntList>::deserialize_from(data)?,
            hints: Vec::<(usize, Vec<Hint>)>::deserialize_from(data)?,
            pythonic_hints: Option::<Vec<(usize, Vec<String>)>>::deserialize_from(data)?,
            entry_points_by_type: CasmContractEntryPoints::deserialize_from(data)?,
        })
    }
}

#[cfg(test)]
create_storage_serde_test!(CasmContractClass);

// The following structs are conditionally compressed based on their serialized size.
macro_rules! auto_storage_serde_conditionally_compressed {
    () => {};
    ($(pub)? struct $name:ident { $(pub $field:ident : $ty:ty ,)* } $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                let mut to_compress: Vec<u8> = Vec::new();
                $(
                    self.$field.serialize_into(&mut to_compress)?;
                )*
                if to_compress.len() > COMPRESSION_THRESHOLD_BYTES {
                    IsCompressed::Yes.serialize_into(res)?;
                    let compressed = compress(to_compress.as_slice())?;
                    compressed.serialize_into(res)?;
                } else {
                    IsCompressed::No.serialize_into(res)?;
                    to_compress.serialize_into(res)?;
                }
                Ok(())
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                let is_compressed = IsCompressed::deserialize_from(bytes)?;
                let maybe_compressed_data = Vec::<u8>::deserialize_from(bytes)?;
                let data = match is_compressed {
                    IsCompressed::No => maybe_compressed_data,
                    IsCompressed::Yes => decompress(maybe_compressed_data.as_slice()).ok()?,
                };
                let data = &mut data.as_slice();
                Some(Self {
                    $(
                        $field: <$ty>::deserialize_from(data)?,
                    )*
                })
            }
        }
        #[cfg(test)]
        create_storage_serde_test!($name);
        auto_storage_serde_conditionally_compressed!($($rest)*);
    };
}

// The following transactions have variable length Calldata and are conditionally compressed.
auto_storage_serde_conditionally_compressed! {
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
}
