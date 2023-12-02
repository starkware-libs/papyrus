use std::fmt::Debug;

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

pub(crate) trait VersionedStorageSerde<TK: TableVersion>: StorageSerde + Debug {
    fn versioned_serialize_into(
        &self,
        version: u8,
        res: &mut impl std::io::Write,
    ) -> Result<(), StorageSerdeError> {
        res.write_all(&[version])?;
        self.serialize_into(res)
    }

    fn versioned_deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let mut version = [0u8; 1];
        bytes.read_exact(&mut version[..]).ok()?;
        let res = Self::deserialize_from_version(version[0], bytes)?;
        Some(res)
    }

    fn deserialize_from_version(version: u8, _bytes: &mut impl std::io::Read) -> Option<Self>;

    fn serialize(&self) -> Result<Vec<u8>, DbError> {
        let mut res: Vec<u8> = Vec::new();
        match TK::VERSION {
            Some(version) => {
                self.versioned_serialize_into(version, &mut res)
                    .map_err(|_| DbError::Serialization)?;
            }
            None => self.serialize_into(&mut res).map_err(|_| DbError::Serialization)?,
        }
        Ok(res)
    }

    fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        let res = match TK::VERSION {
            Some(_) => Self::versioned_deserialize_from(bytes),
            None => Self::deserialize_from(bytes),
        }?;
        let mut buf = [0u8, 1];
        // Make sure we are at EOF.
        if bytes.read(&mut buf[..]).ok()? != 0 {
            return None;
        }
        Some(res)
    }
}

pub(crate) trait TableVersion {
    const VERSION: Option<u8>;
}
#[derive(Clone, Debug)]
pub(crate) struct Version0;
impl TableVersion for Version0 {
    const VERSION: Option<u8> = Some(0);
}

#[derive(Clone, Debug)]
pub(crate) struct UnVersioned;
impl TableVersion for UnVersioned {
    const VERSION: Option<u8> = None;
}
impl<T> VersionedStorageSerde<UnVersioned> for T
where
    T: StorageSerde + Debug,
{
    fn deserialize_from_version(_version: u8, _bytes: &mut impl std::io::Read) -> Option<Self> {
        todo!()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum StorageSerdeError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
