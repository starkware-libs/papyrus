#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use std::io::Read;
use std::marker::PhantomData;

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

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CompressedObject<I>(String, PhantomData<I>);

impl<'a, I> CompressedObject<I>
where
    I: Deserialize<'a> + Serialize + Sized,
{
    // Returns a gzip compression of a given item.
    pub fn encode(item: I) -> Result<Self, CompressionError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        serde_json::to_writer(&mut encoder, &item)?;
        let bytes = encoder.finish()?;
        Ok(CompressedObject(base64::encode(bytes), PhantomData))
    }

    // Returns a decompressed item.
    pub fn decode(&self, buff: &'a mut Vec<u8>) -> Result<I, CompressionError> {
        let bytes = base64::decode(&self.0)?;
        let mut decoder = GzDecoder::new(bytes.as_slice());
        decoder.read_to_end(buff)?;
        Ok(serde_json::from_slice(buff)?)
    }
}
