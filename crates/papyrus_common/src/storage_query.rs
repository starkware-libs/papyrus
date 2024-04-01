//! Contains only the [StorageQuery] struct.
//!
//! The struct is used in the storage_benchmark binary and in the document_calls feature of the
//! [papyrus_storage] library. It is not part of the latter because it is not in
//! use without the document_calls feature enabled.
//!
//! [papyrus_storage]: https://docs.rs/papyrus_storage/latest/papyrus_storage/

// TODO(dvir): add links to the document for the storage_benchmark binary and the
// document_calls feature after they will be publish.

use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use starknet_api::state::{StateNumber, StorageKey};

/// A storage query. Used for benchmarking in the storage_benchmark binary and in the document_calls
/// feature of the [papyrus_storage](https://docs.rs/papyrus_storage/latest/papyrus_storage/).
// TODO(dvir): add more queries (especially get casm).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageQuery {
    /// Get the class hash at a given state number.
    GetClassHashAt(StateNumber, ContractAddress),
    /// Get the nonce at a given state number.
    GetNonceAt(StateNumber, ContractAddress),
    /// Get the storage at a given state number.
    GetStorageAt(StateNumber, ContractAddress, StorageKey),
}
