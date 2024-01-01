use std::fmt::Debug;
use std::marker::PhantomData;

use crate::db::DbError;

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

pub trait StorageSerde: Sized {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError>;

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self>;
}

pub(crate) trait Key: StorageSerdeEx + Ord + Clone {}
impl<T> Key for T where T: StorageSerdeEx + Ord + Clone {}

pub(crate) trait ValueSerde {
    type Value: StorageSerde + Debug;

    fn serialize(obj: &Self::Value) -> Result<Vec<u8>, DbError>;
    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self::Value>;
}

#[derive(Debug)]
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

#[derive(thiserror::Error, Debug)]
pub enum StorageSerdeError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
