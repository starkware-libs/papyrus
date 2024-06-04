#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use crate::db::serialization::{StorageSerde, StorageSerdeError};

// TODO(dvir): create one compressor/decompressor only once (maybe only once per thread) to prevent
// buffer reallocation.
// TODO: fine tune the compression hyperparameters (and maybe even the compression algorithm).

// The maximum size of the decompressed data.
const MAX_DECOMPRESSED_SIZE: usize = 1 << 20; // 1MB
// The compression level to use. Higher levels are slower but compress better.
const COMPRESSION_LEVEL: i32 = zstd::DEFAULT_COMPRESSION_LEVEL;

/// Returns the compressed data in a vector.
///
/// # Arguments
/// * data - bytes to compress.
///
/// # Errors
/// Returns [`std::io::Error`] if any read error is encountered.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    zstd::bulk::compress(data, COMPRESSION_LEVEL)
}

/// Serialized and then compress object.
///
/// # Arguments
/// * object - the object to serialize and compress.
///
/// # Errors
/// Returns [`StorageSerdeError`] if any error is encountered in the serialization or compression.
pub fn serialize_and_compress(object: &impl StorageSerde) -> Result<Vec<u8>, StorageSerdeError> {
    let mut buf = Vec::new();
    object.serialize_into(&mut buf)?;
    Ok(compress(buf.as_slice())?)
}

/// Decompress data and returns it as bytes in a vector.
///
/// # Arguments
/// * data - bytes to decompress.
///
/// # Errors
/// Returns [`std::io::Error`] if any read error is encountered.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    zstd::bulk::decompress(data, MAX_DECOMPRESSED_SIZE)
}

/// Decompress a vector directly from a reader.
/// In case of successful decompression, the vector will be returned; otherwise, None.
///
/// # Arguments
/// * bytes - bytes to read.
pub fn decompress_from_reader(bytes: &mut impl std::io::Read) -> Option<Vec<u8>> {
    let compressed_data = Vec::<u8>::deserialize_from(bytes)?;
    decompress(compressed_data.as_slice()).ok()
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum IsCompressed {
    No = 0,
    Yes = 1,
}
