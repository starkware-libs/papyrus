use crate::compression_utils::{decode_buffer, GzEncoded};
use crate::db::DbError;

pub(crate) trait StorageSerdeEx: StorageSerde {
    fn serialize(&self) -> Result<Vec<u8>, DbError>;

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self>;
}

impl<T: StorageSerde> StorageSerdeEx for T {
    fn serialize(&self) -> Result<Vec<u8>, DbError> {
        if Self::should_compress() {
            let encoded = GzEncoded::encode(self).map_err(|_| DbError::Serialization)?;
            Ok(encoded.0)
        } else {
            let mut res: Vec<u8> = Vec::new();
            self.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
            Ok(res)
        }
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        if Self::should_compress() {
            let mut input = Vec::new();
            bytes.read_to_end(&mut input).ok()?;
            let mut buff = Vec::new();
            decode_buffer(input.as_slice(), &mut buff).ok()
        } else {
            let res = Self::deserialize_from(bytes)?;
            let mut buf = [0u8, 1];
            // Make sure we are at EOF.
            if bytes.read(&mut buf[..]).ok()? != 0 {
                return None;
            }
            Some(res)
        }
    }
}

pub trait StorageSerde: Sized {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError>;

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self>;

    fn should_compress() -> bool {
        false
    }
}

#[derive(thiserror::Error, Debug)]
pub enum StorageSerdeError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
