//! Interface for handling data that is backed up by mmap file directly.

#[cfg(test)]
mod mmap_file_test;

use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::path::PathBuf;

use memmap2::{MmapMut, MmapOptions};

use crate::db::serialization::{StorageSerde, StorageSerdeEx};

// TODO:
// * Make consts configurable.
// * Add logging.
// * Add error handling.
// * Split commit from insert.

/// The growth step of the corresponding file in bytes.
const GROWTH_STEP: u64 = 1 << 30; // 1GB
/// The maximum size of the memory map in bytes.
const MAX_SIZE: u64 = 1 << 40; // 1TB

/// A trait for writing to a memory mapped file.
pub trait Writer<V: StorageSerde> {
    /// Inserts an object to the file, returns the number of bytes written.
    fn insert(&mut self, offset: usize, val: &V) -> usize;
}

/// A trait for reading from a memory mapped file.
pub trait Reader<V: StorageSerde> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> V;
}

/// Represents a location in the file.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LocationInFile {
    /// Offset in the file.
    pub offset: usize,
    /// Length of the object.
    pub len: usize,
}

/// A wrapper around `MMapFile` that provides a write interface.
pub struct FileWriter<V: StorageSerde> {
    large_file: MMapFile<V>,
}
impl<V: StorageSerde> FileWriter<V> {
    pub(crate) fn flush(&self) {
        self.large_file.flush();
    }
}

impl<V: StorageSerde> Writer<V> for FileWriter<V> {
    /// Inserts an object to the file, returns the number of bytes written.
    fn insert(&mut self, offset: usize, val: &V) -> usize {
        self.large_file.insert(offset, val)
    }
}

impl<V: StorageSerde> Reader<V> for FileWriter<V> {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> V {
        self.large_file.get(location)
    }
}

/// A wrapper around `MMapFile` that provides a read interface.
#[derive(Clone)]
pub struct FileReader {
    shared_data: *const u8,
}
unsafe impl Send for FileReader {}
unsafe impl Sync for FileReader {}

impl<V: StorageSerde> Reader<V> for FileReader {
    /// Returns an object from the file.
    fn get(&self, location: LocationInFile) -> V {
        let bytes = std::borrow::Cow::from(unsafe {
            std::slice::from_raw_parts(self.shared_data, location.len)
        });
        V::deserialize(&mut bytes.as_ref()).unwrap()
    }
}

/// Represents an mmap append only file.
pub struct MMapFile<V: StorageSerde> {
    file: File,
    size: usize,
    mmap: MmapMut,
    _value_type: PhantomData<V>,
}

/// Open a mmaped file, create it if it doesn't exist.
pub(crate) fn open_file<V: StorageSerde>(path: PathBuf) -> (FileWriter<V>, FileReader) {
    let file = OpenOptions::new().read(true).write(true).create(true).open(path).unwrap();
    let size = file.metadata().unwrap().len();
    let mmap =
        unsafe { MmapOptions::new().len(MAX_SIZE.try_into().unwrap()).map_mut(&file).unwrap() };
    let l_file =
        MMapFile { file, mmap, size: size.try_into().unwrap(), _value_type: PhantomData {} };
    let reader = FileReader { shared_data: l_file.mmap.as_ptr() };
    let writer = FileWriter { large_file: l_file };
    (writer, reader)
}

impl<V: StorageSerde> MMapFile<V> {
    /// Returns an object from the file.
    pub fn get(&self, location: LocationInFile) -> V {
        let bytes: std::borrow::Cow<'_, [u8]> = self.get_raw(location);
        let val = V::deserialize(&mut bytes.as_ref()).unwrap();
        val
    }

    /// Inserts an object to the file, returns the number of bytes written.
    pub fn insert(&mut self, offset: usize, val: &V) -> usize {
        let bytes = val.serialize().unwrap();
        self.insert_raw(offset, &bytes);
        bytes.len()
    }

    /// Returns a COW pointer to a slice of the file.
    fn get_raw(&self, location: LocationInFile) -> std::borrow::Cow<'_, [u8]> {
        std::borrow::Cow::from(&self.mmap[location.offset..(location.offset + location.len)])
    }

    /// Inserts data to the file,
    fn insert_raw(&mut self, offset: usize, data: &[u8]) {
        while self.size <= offset + data.len() {
            self.grow();
        }
        self.mmap[offset..(offset + data.len())].copy_from_slice(data);
        self.mmap
            .flush_async_range(offset, data.len())
            .expect("Failed to asynchronously flush the mmap after inserting");
    }

    /// Flushes the mmap to the file and grows the file by `GROWTH_STEP`.
    fn grow(&mut self) {
        self.mmap.flush().expect("Failed to flush the mmap before growing");
        let new_size = self.size as u64 + GROWTH_STEP;
        self.file.set_len(new_size).expect("Failed to grow the file");
        self.size = new_size.try_into().unwrap();
    }

    /// Flushes the mmap to the file.
    pub fn flush(&self) {
        self.mmap.flush().expect("Failed to flush the mmap");
    }
}
