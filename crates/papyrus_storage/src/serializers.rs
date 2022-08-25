use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::db::serialization::StorageSerde;

// Implement StorageSerde using bincode.
// TODO(spapini): Replace with custom serializers.
impl<T: Serialize + DeserializeOwned> StorageSerde for T {
    fn serialize_into(&self, res: &mut impl std::io::Write) {
        bincode::serialize_into(res, self).unwrap();
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        bincode::deserialize_from(bytes).ok()
    }
}
