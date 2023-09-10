//! Interface for handling large data that is backed up by mmap file directly.

#[cfg(test)]
mod db_test;

use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::path::PathBuf;

use memmap2::{MmapMut, MmapOptions};

use crate::db::serialization::{StorageSerde, StorageSerdeEx};

/// doc here.
const GROWTH_STEP: u64 = 1 << 30; // 1GB
/// doc here.
const LEN: u64 = 1 << 40; // 1TB

/// Represents an mmap append only file.
pub struct LargeFile<V: StorageSerde> {
    file: File,
    size: usize,
    mmap: MmapMut,
    _value_type: PhantomData<V>,
}

/// Open a mmaped file, create it if it doesn't exist.
pub(crate) fn open_mmaped_file<V: StorageSerde>(path: PathBuf) -> LargeFile<V> {
    let file = OpenOptions::new().read(true).write(true).create(true).open(path).unwrap();
    let size = file.metadata().unwrap().len();
    let mmap = unsafe { MmapOptions::new().len(LEN.try_into().unwrap()).map_mut(&file).unwrap() };
    LargeFile { file, mmap, size: size.try_into().unwrap(), _value_type: PhantomData {} }
}

impl<V: StorageSerde> LargeFile<V> {
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
    pub fn insert_raw(&mut self, offset: usize, data: &[u8]) {
        while self.size <= offset + data.len() {
            self.grow();
        }
        self.mmap[offset..(offset + data.len())].copy_from_slice(data);
        self.mmap.flush().expect("Failed to flush the mmap after inserting");
    }

    /// Flushes the mmap to the file and grows the file by `GROWTH_STEP`.
    fn grow(&mut self) {
        self.mmap.flush().expect("Failed to flush the mmap before growing");
        let new_size = self.size as u64 + GROWTH_STEP;
        self.file.set_len(new_size).expect("Failed to grow the file");
        self.size = new_size.try_into().unwrap();
    }
}

/// Represents a location in the file.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LocationInFile {
    /// Offset in the file.
    pub offset: usize,
    /// Length of the object.
    pub len: usize,
}
