#[cfg(test)]
#[path = "serializers_test.rs"]
mod serializers_test;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use indexmap::IndexMap;
use integer_encoding::*;
use starknet_api::block::{
    BlockHash, BlockHeader, BlockNumber, BlockStatus, BlockTimestamp, GasPrice,
};
use starknet_api::core::{
    ClassHash, ContractAddress, EntryPointSelector, GlobalRoot, Nonce, PatriciaKey,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{
    ContractClass, ContractClassAbiEntry, EntryPoint, EntryPointOffset, EntryPointType,
    EventAbiEntry, FunctionAbiEntry, FunctionAbiEntryType, FunctionAbiEntryWithType, Program,
    StorageKey, StructAbiEntry, StructMember, TypedParameter,
};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, DeclareTransaction, DeployAccountTransaction, DeployTransaction,
    EthAddress, EventContent, EventData, EventIndexInTransactionOutput, EventKey, Fee,
    InvokeTransaction, L1HandlerTransaction, L1ToL2Payload, L2ToL1Payload, MessageToL1,
    MessageToL2, Transaction, TransactionHash, TransactionOffsetInBlock, TransactionSignature,
    TransactionVersion,
};

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput, ThinTransactionOutput,
};
use crate::db::serialization::{StorageSerde, StorageSerdeError};
#[cfg(test)]
use crate::serializers::serializers_test::{create_storage_serde_test, StorageSerdeTest};
use crate::state::data::{IndexedDeclaredContract, IndexedDeployedContract, ThinStateDiff};
use crate::{EventIndex, MarkerKind, OmmerEventKey, OmmerTransactionKey, TransactionIndex};

auto_storage_serde! {
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
    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
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
    pub struct IndexedDeclaredContract {
        pub block_number: BlockNumber,
        pub contract_class: ContractClass,
    }
    pub struct IndexedDeployedContract {
        pub block_number: BlockNumber,
        pub class_hash: ClassHash,
    }
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
    pub struct L1ToL2Payload(pub Vec<StarkFelt>);
    pub struct L2ToL1Payload(pub Vec<StarkFelt>);
    enum MarkerKind {
        Header = 0,
        Body = 1,
        State = 2,
    }
    pub struct MessageToL1 {
        pub to_address: EthAddress,
        pub payload: L2ToL1Payload,
    }
    pub struct MessageToL2 {
        pub from_address: EthAddress,
        pub payload: L1ToL2Payload,
    }
    pub struct Nonce(pub StarkFelt);
    struct OmmerTransactionKey(pub BlockHash, pub TransactionOffsetInBlock);
    struct OmmerEventKey(pub OmmerTransactionKey, pub EventIndexInTransactionOutput);
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
    pub struct StructAbiEntry {
        pub name: String,
        pub size: usize,
        pub members: Vec<StructMember>,
    }
    pub struct StructMember {
        pub param: TypedParameter,
        pub offset: usize,
    }
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
    pub struct TypedParameter {
        pub name: String,
        pub r#type: String,
    }
    pub struct ThinInvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct L1HandlerTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub nonce: Nonce,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
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
    pub enum Transaction {
        Declare(DeclareTransaction) = 0,
        Deploy(DeployTransaction) = 1,
        DeployAccount(DeployAccountTransaction) = 2,
        Invoke(InvokeTransaction) = 3,
        L1Handler(L1HandlerTransaction) = 4,
    }
    pub struct TransactionHash(pub StarkHash);
    struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);
    pub struct TransactionOffsetInBlock(pub usize);
    pub struct TransactionSignature(pub Vec<StarkFelt>);
    pub struct TransactionVersion(pub StarkFelt);

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
    (ContractAddress, EventIndex);
    (ContractAddress, OmmerEventKey);
    (ContractAddress, StorageKey, BlockHash);
    (ContractAddress, StorageKey, BlockNumber);
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
    (bincode($name:ident); $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
                bincode::serialize_into(res, self)?;
                Ok(())
            }

            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                bincode::deserialize_from(bytes).ok()
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
