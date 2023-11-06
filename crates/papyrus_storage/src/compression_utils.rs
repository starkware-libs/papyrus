#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use std::io::Read;

use flate2::bufread::{GzDecoder, GzEncoder};
use flate2::Compression;

use crate::db::serialization::{StorageSerde, StorageSerdeError};

// TODO: consider changing the compression hyperparameters: compression level and algorithm.

/// Returns the compressed data in a vector.
///
/// # Arguments
/// * data - bytes to compress.
///
/// # Errors
/// Returns [`std::io::Error`] if any read error is encountered.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = GzEncoder::new(data, Compression::default());
    let mut compressed_data = Vec::new();
    encoder.read_to_end(&mut compressed_data)?;
    Ok(compressed_data)
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
    let mut decoder = GzDecoder::new(data);
    let mut uncompressed = Vec::new();
    decoder.read_to_end(&mut uncompressed)?;
    Ok(uncompressed)
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
