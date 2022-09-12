#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use std::io::Read;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum CompressionError {
    #[error(transparent)]
    DecodeError(#[from] base64::DecodeError),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}

// Returns a gzip compression of a given item.
pub fn encode(item: impl Serialize + Sized) -> Result<String, CompressionError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    serde_json::to_writer(&mut encoder, &item)?;
    let bytes = encoder.finish()?;
    Ok(base64::encode(bytes))
}

// Returns a decompressed item.
pub fn decode<'de, I>(input: String, buff: &'de mut Vec<u8>) -> Result<I, CompressionError>
where
    I: Sized + Deserialize<'de>,
{
    let bytes = base64::decode(input)?;
    let mut decoder = GzDecoder::new(bytes.as_slice());
    decoder.read_to_end(buff)?;
    Ok(serde_json::from_slice(buff)?)
}
