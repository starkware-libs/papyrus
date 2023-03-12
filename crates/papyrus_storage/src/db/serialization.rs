#[cfg(test)]
#[path = "serialization_test.rs"]
mod serialization_test;

use std::fmt::Debug;
use std::io::Write;

use tracing::trace;

use crate::compression_utils::{decode_buffer, GzEncoded};
use crate::db::DbError;

pub(crate) trait StorageSerdeEx: StorageSerde {
    fn serialize(&self) -> Result<Vec<u8>, DbError>;

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self>;
}

impl<T: StorageSerde> StorageSerdeEx for T {
    fn serialize(&self) -> Result<Vec<u8>, DbError> {
        match Self::should_compress() {
            ShouldCompressOptions::Yes => {
                trace!("Compressing.");
                let encoded = GzEncoded::encode(self).map_err(|_| DbError::Serialization)?;
                Ok(encoded.0)
            }
            ShouldCompressOptions::No => {
                trace!("Not compressing.");
                let mut res: Vec<u8> = Vec::new();
                self.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
                Ok(res)
            }
            ShouldCompressOptions::Maybe => {
                if self.is_big() {
                    trace!("Compressing (big enough).");
                    let mut res: Vec<u8> = Vec::new();
                    res.write_all(&[1]).map_err(|_| DbError::Serialization)?;
                    let encoded = GzEncoded::encode(self).map_err(|_| DbError::Serialization)?;
                    res.write_all(&encoded.0).map_err(|_| DbError::Serialization)?;
                    Ok(res)
                } else {
                    trace!("Not compressing (too small).");
                    let mut res: Vec<u8> = Vec::new();
                    res.write_all(&[0]).map_err(|_| DbError::Serialization)?;
                    self.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
                    Ok(res)
                }
            }
        }
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        match Self::should_compress() {
            ShouldCompressOptions::Yes => deserialize_compressed(bytes),
            ShouldCompressOptions::No => deserialize_uncompressed(bytes),
            ShouldCompressOptions::Maybe => {
                let mut exists = [0u8; 1];
                bytes.read_exact(&mut exists).ok()?;
                match exists[0] {
                    0 => deserialize_uncompressed(bytes),
                    1 => deserialize_compressed(bytes),
                    _ => None,
                }
            }
        }
    }
}

fn deserialize_compressed<T: StorageSerde>(bytes: &mut impl std::io::Read) -> Option<T> {
    let mut input = Vec::new();
    bytes.read_to_end(&mut input).ok()?;
    let mut buff = Vec::new();
    decode_buffer(input.as_slice(), &mut buff).ok()
}

fn deserialize_uncompressed<T: StorageSerde>(bytes: &mut impl std::io::Read) -> Option<T> {
    let res = T::deserialize_from(bytes)?;
    let mut buf = [0u8, 1];
    // Make sure we are at EOF.
    if bytes.read(&mut buf[..]).ok()? != 0 {
        return None;
    }
    Some(res)
}

pub trait StorageSerde: Sized {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError>;

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self>;

    fn should_compress() -> ShouldCompressOptions {
        ShouldCompressOptions::No
    }

    fn is_big(&self) -> bool {
        let mut bytes = Vec::new();
        let _res = self.serialize_into(&mut bytes);
        // TODO(anatg): Insert 500 to the config.
        bytes.len() > 500
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShouldCompressOptions {
    Yes,
    No,
    Maybe,
}

#[derive(thiserror::Error, Debug)]
pub enum StorageSerdeError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
