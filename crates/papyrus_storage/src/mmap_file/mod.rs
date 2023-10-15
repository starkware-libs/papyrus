//! Interface for handling append only data that is backed up by mmap file directly.
//! Data is serialized directly into the mmap file.
//! The caller **must** ensure that:
//! * The serialized data is not larger than the maximum object size.
//! * New data is appended to the file (i.e, at the offset returned by the previous write).

#[cfg(test)]
mod mmap_file_test;

use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::result;
use std::sync::{Arc, Mutex};

use memmap2::{MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use validator::{Validate, ValidationError};

use crate::db::serialization::{StorageSerde, StorageSerdeEx};

#[allow(dead_code)]
type MmapFileResult<V> = result::Result<V, MMapFileError>;

/// Configuration for a memory mapped file.
#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
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

/// Errors associated with [`MMapFile`].
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
pub trait Writer<V: StorageSerde> {
    /// Inserts an object to the file, returns the number of bytes written.
    fn insert(&mut self, offset: usize, val: &V) -> usize;

    /// Flushes the mmap to the file.
    fn flush(&self);
}

/// A trait for reading from a memory mapped file.
pub trait Reader<V: StorageSerde> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> MmapFileResult<Option<V>>;
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

/// A wrapper around `MMapFile` that provides both write and read interfaces.
#[derive(Clone, Debug)]
pub struct FileHandler<V: StorageSerde> {
    memory_ptr: *const u8,
    mmap_file: Arc<Mutex<MMapFile<V>>>,
}

impl<V: StorageSerde> FileHandler<V> {
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

impl<V: StorageSerde + Debug> Writer<V> for FileHandler<V> {
    fn insert(&mut self, offset: usize, val: &V) -> usize {
        debug!("Inserting object at offset: {}", offset);
        trace!("Inserting object: {:?}", val);
        // TODO(dan): change serialize_into to return serialization size.
        let len = val.serialize().expect("Should be able to serialize").len();
        {
            let mut mmap_file = self.mmap_file.lock().expect("Lock should not be poisoned");
            let mut mmap_slice = &mut mmap_file.mmap[offset..];
            let _ = val.serialize_into(&mut mmap_slice);
            mmap_file
                .mmap
                .flush_async_range(offset, len)
                .expect("Failed to asynchronously flush the mmap after inserting");
        }
        self.grow_file_if_needed(offset + len);
        len
    }

    fn flush(&self) {
        let mmap_file = self.mmap_file.lock().expect("Lock should not be poisoned");
        mmap_file.flush();
    }
}

impl<V: StorageSerde> Reader<V> for FileHandler<V> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> MmapFileResult<Option<V>> {
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

/// A wrapper around `FileHandler` that provides a write interface.
#[derive(Debug)]
pub struct FileWriter<V: StorageSerde> {
    file_handler: FileHandler<V>,
}

impl<V: StorageSerde + Debug> Writer<V> for FileWriter<V> {
    fn insert(&mut self, offset: usize, val: &V) -> usize {
        self.file_handler.insert(offset, val)
    }

    fn flush(&self) {
        self.file_handler.flush();
    }
}

impl<V: StorageSerde> Reader<V> for FileWriter<V> {
    fn get(&self, location: LocationInFile) -> MmapFileResult<Option<V>> {
        self.file_handler.get(location)
    }
}

/// A wrapper around `FileHandler` that provides a read interface.
#[derive(Clone, Debug)]
pub struct FileReader<V: StorageSerde> {
    file_handler: FileHandler<V>,
}
unsafe impl<V: StorageSerde> Send for FileReader<V> {}
unsafe impl<V: StorageSerde> Sync for FileReader<V> {}

impl<V: StorageSerde> Reader<V> for FileReader<V> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> MmapFileResult<Option<V>> {
        self.file_handler.get(location)
    }
}

/// Represents a memory mapped append only file.
#[derive(Debug)]
pub struct MMapFile<V: StorageSerde> {
    config: MmapFileConfig,
    file: File,
    size: usize,
    mmap: MmapMut,
    _value_type: PhantomData<V>,
}

impl<V: StorageSerde> MMapFile<V> {
    /// Grows the file by the growth step.
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
) -> MmapFileResult<(FileWriter<V>, FileReader<V>)> {
    debug!("Opening file");
    // TODO: move validation to caller.
    config.validate().expect("Invalid config");
    let file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
    let size = file.metadata()?.len();
    let mmap = unsafe { MmapOptions::new().len(config.max_size).map_mut(&file)? };
    let mmap_ptr = mmap.as_ptr();
    let mmap_file = MMapFile {
        config,
        file,
        mmap,
        size: size.try_into().expect("size should fit in usize"),
        _value_type: PhantomData {},
    };
    let shared_mmap_file = Arc::new(Mutex::new(mmap_file));

    let mut file_handler =
        FileHandler { memory_ptr: mmap_ptr, mmap_file: shared_mmap_file.clone() };
    file_handler.grow_file_if_needed(0);
    let writer = FileWriter { file_handler };

    let file_handler = FileHandler { memory_ptr: mmap_ptr, mmap_file: shared_mmap_file };
    let reader = FileReader { file_handler };

    Ok((writer, reader))
}
