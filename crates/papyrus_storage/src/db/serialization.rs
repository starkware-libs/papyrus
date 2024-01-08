use std::fmt::Debug;
use std::io::Write;
use std::marker::PhantomData;

use crate::db::DbError;

/// Trait for serializing and deserializing values.
pub(crate) trait StorageSerdeEx: StorageSerde {
    fn serialize(&self) -> Result<Vec<u8>, DbError>;

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self>;
}

impl<T: StorageSerde> StorageSerdeEx for T {
    fn serialize(&self) -> Result<Vec<u8>, DbError> {
        let mut res: Vec<u8> = Vec::new();
        self.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        Ok(res)
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        let res = Self::deserialize_from(bytes)?;
        let mut buf = [0u8, 1];
        // Make sure we are at EOF.
        if bytes.read(&mut buf[..]).ok()? != 0 {
            return None;
        }
        Some(res)
    }
}

/// Trait for deserializing and serializing values into buffers.
pub trait StorageSerde: Sized {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError>;

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self>;
}

/// Trait that enforces a database key to implement `StorageSerdeEx`, `Ord` and `Clone`.
pub(crate) trait Key: StorageSerdeEx + Ord + Clone {}
impl<T> Key for T where T: StorageSerdeEx + Ord + Clone {}

/// Trait for serializing and deserializing values from the database.
pub(crate) trait ValueSerde {
    type Value: StorageSerde + Debug;

    fn serialize(obj: &Self::Value) -> Result<Vec<u8>, DbError>;
    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self::Value>;
}

#[derive(Debug)]
/// A generic wrapper for values that do not have a version.
pub(crate) struct NoVersionValueWrapper<T: StorageSerde> {
    _value_type: PhantomData<T>,
}

impl<T: StorageSerde + Debug> ValueSerde for NoVersionValueWrapper<T> {
    type Value = T;

    fn serialize(obj: &Self::Value) -> Result<Vec<u8>, DbError> {
        StorageSerdeEx::serialize(obj)
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self::Value> {
        StorageSerdeEx::deserialize(bytes)
    }
}

#[derive(Debug)]
/// A generic wrapper for values with version zero. These values are serialized with a leading byte
/// that is set to zero.
pub(crate) struct VersionZeroWrapper<T: StorageSerde> {
    _value_type: PhantomData<T>,
}

const VERSION_ZERO: u8 = 0;

impl<T: StorageSerde + Debug> ValueSerde for VersionZeroWrapper<T> {
    type Value = T;

    fn serialize(obj: &Self::Value) -> Result<Vec<u8>, DbError> {
        let mut res = Vec::new();
        res.write_all(&[VERSION_ZERO]).expect("Failed to write version");
        obj.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        Ok(res)
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self::Value> {
        let mut version = [0u8; 1];
        bytes.read_exact(&mut version[..]).ok()?;
        if version[0] != VERSION_ZERO {
            return None;
        }
        let res = Self::Value::deserialize_from(bytes)?;

        let mut buf = [0u8, 1];
        // Make sure we are at EOF.
        if bytes.read(&mut buf[..]).ok()? != 0 {
            return None;
        }
        Some(res)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum StorageSerdeError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
