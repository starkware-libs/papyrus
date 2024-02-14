#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use papyrus_storage::compression_utils::serialize_and_compress;
use papyrus_storage::serialization::serialization_traits::{StorageSerde, StorageSerdeError};

pub fn compress_and_encode(value: serde_json::Value) -> Result<String, StorageSerdeError> {
    Ok(base64::encode(serialize_and_compress(&JsonValue(value))?))
}

// The StorageSerde implementation for serde_json::Value writes the length (in bytes)
// of the value. Here we serialize the whole program as one value so no need to write
// its length.
struct JsonValue(serde_json::Value);
impl StorageSerde for JsonValue {
    /// Serializes the entire program as one json value.
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        serde_json::to_writer(res, &self.0)?;
        Ok(())
    }

    /// Deserializes the entire program as one json value.
    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let value = serde_json::from_reader(bytes).ok()?;
        Some(JsonValue(value))
    }
}
