//! Interface for handling append only data that is backed up by mmap file directly.
//! Data is serialized directly into the mmap file.
//! The caller **must** ensure that:
//! * The serialized data is not larger than the maximum object size.
//! * New data is appended to the file (i.e, at the offset returned by the previous write).

#[cfg(test)]
mod mmap_file_test;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::result;
use std::sync::{Arc, Mutex};

use memmap2::{MmapMut, MmapOptions};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
#[cfg(test)]
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use test_utils::GetTestInstance;
use thiserror::Error;
use tracing::{debug, instrument, trace};
use validator::{Validate, ValidationError};

use crate::db::serialization::{StorageSerde, ValueSerde};
use crate::db::{TransactionKind, RO, RW};

type MmapFileResult<V> = result::Result<V, MMapFileError>;

/// Configuration for a memory mapped file.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
#[validate(schema(function = "validate_config"))]
pub struct MmapFileConfig {
    /// The maximum size of the memory map in bytes.
    pub max_size: usize,
    /// The growth step of the corresponding file in bytes.
    pub growth_step: usize,
    /// The maximum size of an object in bytes.
    pub max_object_size: usize,
}

impl SerializeConfig for MmapFileConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of a memory mapped file in bytes. Must be greater than \
                 growth_step.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "growth_step",
                &self.growth_step,
                "The growth step in bytes, must be greater than max_object_size.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_object_size",
                &self.max_object_size,
                "The maximum size of a single object in the file in bytes",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for MmapFileConfig {
    fn default() -> Self {
        Self {
            max_size: 1 << 40,        // 1TB
            growth_step: 1 << 30,     // 1GB
            max_object_size: 1 << 20, // 1MB
        }
    }
}

fn validate_config(config: &MmapFileConfig) -> result::Result<(), ValidationError> {
    if config.max_size < config.growth_step {
        return Err(ValidationError::new("max_size should be larger than growth_step"));
    }
    if config.growth_step < config.max_object_size {
        return Err(ValidationError::new("growth_step should be larger than max_object_size"));
    }
    Ok(())
}

/// Errors associated with memory mapped files.
#[derive(Debug, Error)]
pub enum MMapFileError {
    /// IO error.
    #[error(transparent)]
    IO(#[from] std::io::Error),

    /// Number conversion error.
    #[error(transparent)]
    TryFromInt(#[from] std::num::TryFromIntError),
}

/// A trait for writing to a memory mapped file.
pub(crate) trait Writer<V: ValueSerde> {
    /// Inserts an object to the file, returns the [`LocationInFile`] of the object.
    fn append(&mut self, val: &V::Value) -> LocationInFile;

    /// Flushes the mmap to the file.
    fn flush(&self);
}

/// A trait for reading from a memory mapped file.
pub(crate) trait Reader<V: ValueSerde> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> MmapFileResult<Option<V::Value>>;
}

/// Represents a location in the file.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LocationInFile {
    /// Offset in the file.
    offset: usize,
    /// Length of the object.
    len: usize,
}

impl LocationInFile {
    /// returns the next offset in the file.
    pub fn next_offset(&self) -> usize {
        self.offset + self.len
    }
}

/// Represents a memory mapped append only file.
#[derive(Debug)]
struct MMapFile<V: ValueSerde> {
    config: MmapFileConfig,
    file: File,
    size: usize,
    mmap: MmapMut,
    offset: usize,
    should_flush: bool,
    _value_type: PhantomData<V>,
}

impl<V: ValueSerde> MMapFile<V> {
    /// Grows the file by the growth step.
    fn grow(&mut self) {
        self.flush();
        let new_size = self.size + self.config.growth_step;
        debug!("Growing file to size: {}", new_size);
        self.file.set_len(new_size as u64).expect("Failed to set the file size");
        self.size = new_size;
    }

    /// Flushes the mmap to the file.
    fn flush(&mut self) {
        debug!("Flushing mmap to file");
        self.mmap.flush().expect("Failed to flush the mmap");
        self.should_flush = false;
    }
}

/// Open a memory mapped file, create it if it doesn't exist.
#[instrument(level = "debug", err)]
pub(crate) fn open_file<V: ValueSerde>(
    config: MmapFileConfig,
    path: PathBuf,
    offset: usize,
) -> MmapFileResult<(FileHandler<V, RW>, FileHandler<V, RO>)> {
    let file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
    let size = file.metadata()?.len();
    let mmap = unsafe { MmapOptions::new().len(config.max_size).map_mut(&file)? };
    let mmap_ptr = mmap.as_ptr();
    let mmap_file = MMapFile {
        config,
        file,
        mmap,
        size: size.try_into().expect("size should fit in usize"),
        offset,
        should_flush: false,
        _value_type: PhantomData {},
    };
    let shared_mmap_file = Arc::new(Mutex::new(mmap_file));

    let mut write_file_handler: FileHandler<V, RW> = FileHandler {
        memory_ptr: mmap_ptr,
        mmap_file: shared_mmap_file.clone(),
        _mode: PhantomData,
    };
    write_file_handler.grow_file_if_needed(0);

    let read_file_handler: FileHandler<V, RO> =
        FileHandler { memory_ptr: mmap_ptr, mmap_file: shared_mmap_file, _mode: PhantomData };

    Ok((write_file_handler, read_file_handler))
}

/// A wrapper around `MMapFile` that provides both write and read interfaces.
#[derive(Clone, Debug)]
pub(crate) struct FileHandler<V: ValueSerde, Mode: TransactionKind> {
    memory_ptr: *const u8,
    mmap_file: Arc<Mutex<MMapFile<V>>>,
    _mode: PhantomData<Mode>,
}

unsafe impl<V: ValueSerde, Mode: TransactionKind> Send for FileHandler<V, Mode> {}
unsafe impl<V: ValueSerde, Mode: TransactionKind> Sync for FileHandler<V, Mode> {}

impl<V: ValueSerde> FileHandler<V, RW> {
    fn grow_file_if_needed(&mut self, offset: usize) {
        let mut mmap_file = self.mmap_file.lock().expect("Lock should not be poisoned");
        if mmap_file.size < offset + mmap_file.config.max_object_size {
            debug!(
                "Attempting to grow file. File size: {}, offset: {}, max_object_size: {}",
                mmap_file.size, offset, mmap_file.config.max_object_size
            );
            mmap_file.grow();
        }
    }
}

impl<V: ValueSerde + Debug> Writer<V> for FileHandler<V, RW> {
    fn append(&mut self, val: &V::Value) -> LocationInFile {
        trace!("Inserting object: {:?}", val);
        let serialized = V::serialize(val).expect("Should be able to serialize");
        let len = serialized.len();
        let offset;
        {
            let mut mmap_file = self.mmap_file.lock().expect("Lock should not be poisoned");
            offset = mmap_file.offset;
            debug!("Inserting object at offset: {}", offset);
            let mmap_slice = &mut mmap_file.mmap[offset..];
            mmap_slice[..len].copy_from_slice(&serialized);
            mmap_file
                .mmap
                .flush_async_range(offset, len)
                .expect("Failed to asynchronously flush the mmap after inserting");
            mmap_file.offset += len;
            mmap_file.should_flush = true;
        }
        let location = LocationInFile { offset, len };
        self.grow_file_if_needed(location.next_offset());
        location
    }

    fn flush(&self) {
        let mut mmap_file = self.mmap_file.lock().expect("Lock should not be poisoned");
        if mmap_file.should_flush {
            mmap_file.flush();
        }
    }
}

impl<V: ValueSerde, Mode: TransactionKind> Reader<V> for FileHandler<V, Mode> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> MmapFileResult<Option<V::Value>> {
        debug!("Reading object at location: {:?}", location);
        let mut bytes = unsafe {
            std::slice::from_raw_parts(
                self.memory_ptr.offset(location.offset.try_into()?),
                location.len,
            )
        };
        trace!("Deserializing object: {:?}", bytes);
        Ok(V::deserialize(&mut bytes))
    }
}

// TODO(dan): use varint serialization.
impl StorageSerde for LocationInFile {
    fn serialize_into(
        &self,
        res: &mut impl std::io::Write,
    ) -> Result<(), crate::db::serialization::StorageSerdeError> {
        self.offset.serialize_into(res)?;
        self.len.serialize_into(res)?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let offset = usize::deserialize_from(bytes)?;
        let len = usize::deserialize_from(bytes)?;
        Some(Self { offset, len })
    }
}

#[cfg(test)]
impl GetTestInstance for LocationInFile {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self { offset: usize::get_test_instance(rng), len: usize::get_test_instance(rng) }
    }
}
