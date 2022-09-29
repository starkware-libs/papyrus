#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use std::io::Read;
use std::marker::PhantomData;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

/// Errors that may be returned when encoding or decoding with one of the functions in this file.
#[derive(thiserror::Error, Debug)]
pub enum CompressionError {
    /// An error representing reading and writing errors.
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// An error representing serialization and deserialization errors.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// An object that was encoded with [`GzEncoder`].
/// The phantom data represents the type of the object that was encoded.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct GzEncoded<I>(Vec<u8>, PhantomData<I>);

impl<'a, I> GzEncoded<I>
where
    I: Deserialize<'a> + Serialize + Sized,
{
    /// Returns a gzip compression of a given item.
    pub fn encode(item: I) -> Result<Self, CompressionError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        serde_json::to_writer(&mut encoder, &item)?;
        let bytes = encoder.finish()?;
        Ok(Self(bytes, PhantomData))
    }

    /// Returns a decompressed item.
    pub fn decode(&self, buff: &'a mut Vec<u8>) -> Result<I, CompressionError> {
        let mut decoder = GzDecoder::new(self.0.as_slice());
        decoder.read_to_end(buff)?;
        Ok(serde_json::from_slice(buff)?)
    }
}

impl<I> AsRef<[u8]> for GzEncoded<I> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
