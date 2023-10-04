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

use memmap2::{MmapMut, MmapOptions};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use validator::{Validate, ValidationError};

use crate::db::serialization::{StorageSerde, StorageSerdeEx};

#[allow(dead_code)]
type MmapFileResult<V> = result::Result<V, MMapFileError>;

/// Configuration for a memory mapped file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_config"))]
pub struct MmapFileConfig {
    /// The maximum size of the memory map in bytes.
    pub max_size: usize,
    /// The growth step of the corresponding file in bytes.
    pub growth_step: usize,
    /// The maximum size of an object in bytes.
    pub max_object_size: usize,
}

impl Default for MmapFileConfig {
    fn default() -> Self {
        Self {
            max_size: 1 << 37,        // 128GB
            growth_step: 1 << 30,     // 1GB
            max_object_size: 1 << 26, // 64MB
        }
    }
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

fn validate_config(config: &MmapFileConfig) -> result::Result<(), ValidationError> {
    if config.max_size < config.growth_step {
        return Err(ValidationError::new("max_size should be larger than growth_step"));
    }
    if config.growth_step < config.max_object_size {
        return Err(ValidationError::new("growth_step should be larger than max_object_size"));
    }
    Ok(())
}

/// Errors associated with [`MMapFile`].
#[derive(Debug, Error)]
pub enum MMapFileError {
    #[error(transparent)]
    /// IO error.
    IO(#[from] std::io::Error),
}

/// A trait for writing to a memory mapped file.
pub trait Writer<V: StorageSerde> {
    /// Inserts an object to the file, returns the number of bytes written.
    fn insert(&mut self, offset: usize, val: &V) -> usize;
}

/// A trait for reading from a memory mapped file.
pub trait Reader<V: StorageSerde> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> Option<V>;
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
    #[allow(dead_code)]
    fn next_offset(&self) -> usize {
        self.offset + self.len
    }
}

/// A wrapper around `MMapFile` that provides a write interface.
pub struct FileWriter<V: StorageSerde> {
    mmap_file: MMapFile<V>,
}
impl<V: StorageSerde> FileWriter<V> {
    /// Flushes the mmap to the file.
    #[allow(dead_code)]
    pub(crate) fn flush(&self) {
        self.mmap_file.flush();
    }

    fn grow_file_if_needed(&mut self, offset: usize) {
        if self.mmap_file.size < offset + self.mmap_file.config.max_object_size {
            debug!(
                "Attempting to grow file. File size: {}, offset: {}, max_object_size: {}",
                self.mmap_file.size, offset, self.mmap_file.config.max_object_size
            );
            self.mmap_file.grow();
        }
    }
}

impl<V: StorageSerde + Debug> Writer<V> for FileWriter<V> {
    /// Inserts an object to the file, returns the number of bytes written. Grow file if needed.
    fn insert(&mut self, offset: usize, val: &V) -> usize {
        debug!("Inserting object at offset: {}", offset);
        trace!("Inserting object: {:?}", val);
        let mut mmap_slice = &mut self.mmap_file.mmap[offset..];
        // TODO(dan): change serialize_into to return serialization size.
        let _ = val.serialize_into(&mut mmap_slice);
        let len = val.serialize().expect("Should be able to serialize").len();
        self.mmap_file
            .mmap
            .flush_async_range(offset, len)
            .expect("Failed to asynchronously flush the mmap after inserting");
        self.grow_file_if_needed(offset + len);
        len
    }
}

impl<V: StorageSerde> Reader<V> for FileWriter<V> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> Option<V> {
        self.mmap_file.get(location)
    }
}

/// A wrapper around `MMapFile` that provides a read interface.
#[derive(Clone, Copy, Debug)]
pub struct FileReader {
    shared_data: *const u8,
}
unsafe impl Send for FileReader {}
unsafe impl Sync for FileReader {}

impl<V: StorageSerde> Reader<V> for FileReader {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> Option<V> {
        debug!("Reading object at location: {:?}", location);
        let mut bytes = unsafe {
            std::slice::from_raw_parts(
                self.shared_data
                    .offset(location.offset.try_into().expect("offset should fit in usize")),
                location.len,
            )
        };
        trace!("Deserializing object: {:?}", bytes);
        V::deserialize(&mut bytes)
    }
}

/// Represents a memory mapped append only file.
pub struct MMapFile<V: StorageSerde> {
    config: MmapFileConfig,
    file: File,
    size: usize,
    mmap: MmapMut,
    _value_type: PhantomData<V>,
}

impl<V: StorageSerde> MMapFile<V> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> Option<V> {
        debug!("Reading object at location: {:?}", location);
        let bytes: std::borrow::Cow<'_, [u8]> = self.get_raw(location);
        trace!("Deserializing object: {:?}", bytes.as_ref());
        V::deserialize(&mut bytes.as_ref())
    }

    /// Returns a COW pointer to a slice of the file.
    fn get_raw(&self, location: LocationInFile) -> std::borrow::Cow<'_, [u8]> {
        std::borrow::Cow::from(&self.mmap[location.offset..(location.offset + location.len)])
    }

    fn grow(&mut self) {
        self.flush();
        let new_size = self.size + self.config.growth_step;
        debug!("Growing file to size: {}", new_size);
        self.file.set_len(new_size as u64).expect("Failed to set the file size");
        self.size = new_size;
    }

    /// Flushes the mmap to the file.
    fn flush(&self) {
        debug!("Flushing mmap to file");
        self.mmap.flush().expect("Failed to flush the mmap");
    }
}

/// Open a memory mapped file, create it if it doesn't exist.
#[instrument(level = "debug", err)]
pub(crate) fn open_file<V: StorageSerde>(
    config: MmapFileConfig,
    path: PathBuf,
) -> MmapFileResult<(FileWriter<V>, FileReader)> {
    debug!("Opening file");
    let file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
    let size = file.metadata()?.len();
    let mmap = unsafe { MmapOptions::new().len(config.max_size).map_mut(&file)? };
    let mmap_file = MMapFile {
        config,
        file,
        mmap,
        size: size.try_into().expect("size should fit in usize"),
        _value_type: PhantomData {},
    };
    let reader = FileReader { shared_data: mmap_file.mmap.as_ptr() };
    let mut writer = FileWriter { mmap_file };
    writer.grow_file_if_needed(0);
    Ok((writer, reader))
}
