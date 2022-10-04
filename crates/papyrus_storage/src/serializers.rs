use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::Hash;

use integer_encoding::*;
use starknet_api::{
    BlockHash, BlockHeader, BlockNumber, BlockStatus, BlockTimestamp, CallData, ClassHash,
    ContractAddress, ContractAddressSalt, ContractClass, ContractNonce, DeclareTransaction,
    DeclaredContract, DeployAccountTransaction, DeployTransaction, DeployedContract, EntryPoint,
    EntryPointOffset, EntryPointSelector, EntryPointType, EthAddress, EventContent, EventData,
    EventIndexInTransactionOutput, EventKey, Fee, GasPrice, GlobalRoot, InvokeTransaction,
    L1HandlerTransaction, L1ToL2Payload, L2ToL1Payload, MessageToL1, MessageToL2, Nonce,
    PatriciaKey, Program, StarkFelt, StarkHash, StateDiff, StorageDiff, StorageEntry, StorageKey,
    Transaction, TransactionHash, TransactionOffsetInBlock, TransactionSignature,
    TransactionVersion,
};

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput, ThinTransactionOutput,
};
use crate::db::serialization::StorageSerde;
use crate::state::{IndexedDeclaredContract, IndexedDeployedContract};
use crate::{EventIndex, MarkerKind, ThinStateDiff, TransactionIndex};

////////////////////////////////////////////////////////////////////////
// Starknet API structs.
////////////////////////////////////////////////////////////////////////
impl StorageSerde for ContractAddress {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        self.contract_address().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        ContractAddress::try_from(StarkHash::deserialize(bytes)?).ok()
    }
}

impl StorageSerde for PatriciaKey {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        self.key().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Self::new(StarkHash::deserialize(bytes)?).ok()
    }
}

// TODO(spapini): Perhaps compress this textual data.
impl StorageSerde for serde_json::Value {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        // TODO(anatg): Deal with serde_json error.
        serde_json::to_writer(res, self).unwrap();
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        serde_json::from_reader(bytes).ok()
    }
}

impl StorageSerde for StarkHash {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        self.serialize(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Self::deserialize(bytes)
    }
}

impl StorageSerde for StorageKey {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        self.clone().key().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        StorageKey::try_from(StarkHash::deserialize(bytes)?).ok()
    }
}

impl StorageSerde for TransactionOffsetInBlock {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        (self.0 as u64).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(Self(u64::deserialize_from(bytes)? as usize))
    }
}

// TODO(anatg): Consider using wrapper instead.
impl StorageSerde for EventIndexInTransactionOutput {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        (self.0 as u64).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(Self(u64::deserialize_from(bytes)? as usize))
    }
}

////////////////////////////////////////////////////////////////////////
//  Primitive types.
////////////////////////////////////////////////////////////////////////
impl<T: StorageSerde> StorageSerde for Option<T> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        match self {
            Some(value) => {
                res.write_all(&[1])?;
                value.serialize_into(res)
            }
            None => res.write_all(&[0]),
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
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        res.write_varint(self.len()).expect("I/O error during Vec serialization");
        for x in self {
            x.serialize_into(res)?
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let n: usize = bytes.read_varint().unwrap();
        let mut res = Vec::with_capacity(n as usize);
        for _i in 0..n {
            res.push(T::deserialize_from(bytes)?);
        }
        Some(res)
    }
}
impl<K: StorageSerde + Eq + Hash, V: StorageSerde> StorageSerde for HashMap<K, V> {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        res.write_varint(self.len()).expect("I/O error during HashMap serialization");
        for (k, v) in self.iter() {
            k.serialize_into(res)?;
            v.serialize_into(res)?;
        }
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let n: usize = bytes.read_varint().unwrap();
        let mut res = HashMap::with_capacity(n as usize);
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
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
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

////////////////////////////////////////////////////////////////////////
//  impl StorageSerde macro.
////////////////////////////////////////////////////////////////////////
macro_rules! auto_storage_serde {
    () => {};
    // Tuple structs (no names associated with fields) - one field.
    ($(pub)? struct $name:ident($(pub)? $ty:ty); $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
                self.0.serialize_into(res)
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some(Self (<$ty>::deserialize_from(bytes)?))
            }
        }
        auto_storage_serde!($($rest)*);
    };
    // Tuple structs (no names associated with fields) - two fields.
    ($(pub)? struct $name:ident($(pub)? $ty0:ty, $(pub)? $ty1:ty) ; $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
                self.0.serialize_into(res)?;
                self.1.serialize_into(res)
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some($name(<$ty0>::deserialize_from(bytes)?, <$ty1>::deserialize_from(bytes)?))
            }
        }
        auto_storage_serde!($($rest)*);
    };
    // Structs with public fields.
    ($(pub)? struct $name:ident { $(pub $field:ident : $ty:ty ,)* } $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
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
        auto_storage_serde!($($rest)*);
    };
    // Structs with private fields and getters.
    (wrapper($name:ident, $($field_getter:ident : $ty:ty ,)*); $($rest:tt)* ) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
                $(
                    self.$field_getter().serialize_into(res)?;
                )*
                Ok(())
            }
            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                Some(Self::new(
                    $(<$ty>::deserialize_from(bytes)?,)*
                ))
            }
        }
        auto_storage_serde!($($rest)*);
    };
    // Tuples - two elements.
    (($ty0:ty, $ty1:ty) ; $($rest:tt)*) => {
        impl StorageSerde for ($ty0, $ty1) {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
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
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
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
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
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
        auto_storage_serde!($($rest)*);
    };
    // Binary.
    (bincode($name:ident); $($rest:tt)*) => {
        impl StorageSerde for $name {
            fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
                bincode::serialize_into(res, self).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))
            }

            fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
                bincode::deserialize_from(bytes).ok()
            }
        }
        auto_storage_serde!($($rest)*);
    }
}
// Helper macro.
macro_rules! variant {
    ($value:ident, $variant:ident) => {
        Self::$variant
    };
    ($value:ident, $variant:ident($ty:ty)) => {
        Self::$variant($value)
    };
}

auto_storage_serde! {
    pub struct BlockHeader {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub gas_price: GasPrice,
        pub state_root: GlobalRoot,
        pub sequencer: ContractAddress,
        pub timestamp: BlockTimestamp,
    }
    pub enum BlockStatus {
        Pending = 0,
        AcceptedOnL2 = 1,
        AcceptedOnL1 = 2,
        Rejected = 3,
    }
    pub struct CallData(pub Vec<StarkFelt>);
    pub struct ContractAddressSalt(pub StarkHash);
    pub struct ContractClass {
        pub abi: serde_json::Value,
        pub program: Program,
        pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    }
    pub struct ContractNonce {
        pub contract_address: ContractAddress,
        pub nonce: Nonce,
    }
    pub struct DeclaredContract {
        pub class_hash: ClassHash,
        pub contract_class: ContractClass,
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
    pub struct ThinDeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
    }
    pub struct DeployedContract {
        pub address: ContractAddress,
        pub class_hash: ClassHash,
    }
    pub struct DeployTransaction {
        pub transaction_hash: TransactionHash,
        pub version: TransactionVersion,
        pub class_hash: ClassHash,
        pub contract_address: ContractAddress,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: CallData,
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
    pub struct EntryPoint {
        pub selector: EntryPointSelector,
        pub offset: EntryPointOffset,
    }
    pub struct EntryPointOffset(pub StarkFelt);
    pub struct EntryPointSelector(pub StarkHash);
    pub enum EntryPointType {
        Constructor = 0,
        External = 1,
        L1Handler = 2,
    }
    pub struct EventContent {
        pub keys: Vec<EventKey>,
        pub data: EventData,
    }
    pub struct EventData(pub Vec<StarkFelt>);
    pub struct EventKey(pub StarkFelt);
    pub struct Fee(pub u128);
    pub struct IndexedDeclaredContract {
        pub block_number: BlockNumber,
        pub contract_class: Vec<u8>,
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
        pub contract_address: ContractAddress,
        pub entry_point_selector: Option<EntryPointSelector>,
        pub calldata: CallData,
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
        pub calldata: CallData,
    }
    pub struct ThinL1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
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
    pub struct StorageDiff {
        pub address: ContractAddress,
        pub storage_entries: Vec<StorageEntry>,
    }
    pub struct StorageEntry {
        pub key: StorageKey,
        pub value: StarkFelt,
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
    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
    pub enum ThinTransactionOutput {
        Declare(ThinDeclareTransactionOutput) = 0,
        Deploy(ThinDeployTransactionOutput) = 1,
        DeployAccount(ThinDeployAccountTransactionOutput) = 2,
        Invoke(ThinInvokeTransactionOutput) = 3,
        L1Handler(ThinL1HandlerTransactionOutput) = 4,
    }
    pub struct TransactionVersion(pub StarkFelt);
    pub struct TransactionSignature(pub Vec<StarkFelt>);

    wrapper(BlockHash, block_hash : StarkHash,);
    wrapper(BlockNumber, number : u64,);
    wrapper(BlockTimestamp, time_stamp : u64,);
    wrapper(ClassHash, class_hash : StarkHash,);
    wrapper(GasPrice, price : u128,);
    wrapper(GlobalRoot, root : StarkHash,);
    wrapper(Nonce, nonce : StarkFelt,);

    bincode(EthAddress);
    bincode(u8);
    bincode(u32);
    bincode(u64);
    bincode(u128);

    (BlockNumber, TransactionOffsetInBlock);
    (ContractAddress, BlockNumber);
    (ContractAddress, Nonce);
    (ContractAddress, StorageKey, BlockNumber);
    (ContractAddress, EventIndex);
}

////////////////////////////////////////////////////////////////////////
//  impl StorageSerde for types not supported by the macro.
////////////////////////////////////////////////////////////////////////
impl StorageSerde for ThinStateDiff {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        self.deployed_contracts().serialize_into(res)?;
        self.storage_diffs().serialize_into(res)?;
        self.declared_contract_hashes().serialize_into(res)?;
        self.nonces().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let deployed_contracts = Vec::<DeployedContract>::deserialize_from(bytes)?;
        let storage_diffs = Vec::<StorageDiff>::deserialize_from(bytes)?;
        let declared_contract_hashes = Vec::<ClassHash>::deserialize_from(bytes)?;
        let nonces = Vec::<ContractNonce>::deserialize_from(bytes)?;

        // We create ThinStateDiff from StateDiff. Add dummy contract classes.
        let declared_contracts = declared_contract_hashes
            .into_iter()
            .map(|declared_contract_hashe| DeclaredContract {
                class_hash: declared_contract_hashe,
                contract_class: ContractClass::default(),
            })
            .collect();
        Some(
            StateDiff::new(deployed_contracts, storage_diffs, declared_contracts, nonces)
                .ok()?
                .into(),
        )
    }
}

impl StorageSerde for Program {
    /// Serializes the entire program as one json value.
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        // TODO(anatg): Deal with serde_json error.
        serde_json::to_value(self).unwrap().serialize_into(res)
    }

    /// Deserializes the entire program as one json value.
    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let value = serde_json::Value::deserialize_from(bytes)?;
        serde_json::from_value(value).ok()
    }
}
