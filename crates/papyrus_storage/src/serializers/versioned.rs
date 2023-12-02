use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::block::BlockHeader;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, ThinStateDiff};
use starknet_api::transaction::Transaction;

use crate::body::events::ThinTransactionOutput;
use crate::db::serialization::{StorageSerde, Version0, VersionedStorageSerde};

impl VersionedStorageSerde<Version0> for BlockHeader {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}

impl VersionedStorageSerde<Version0> for Transaction {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}

impl VersionedStorageSerde<Version0> for ThinTransactionOutput {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}

impl VersionedStorageSerde<Version0> for ThinStateDiff {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}
impl VersionedStorageSerde<Version0> for ContractClass {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}

impl VersionedStorageSerde<Version0> for CasmContractClass {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}

impl VersionedStorageSerde<Version0> for DeprecatedContractClass {
    fn deserialize_from_version(version: u8, bytes: &mut impl std::io::Read) -> Option<Self> {
        match version {
            0 => Self::deserialize_from(bytes),
            _ => None,
        }
    }
}
