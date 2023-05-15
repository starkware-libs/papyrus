use std::io::Read;

use flate2::bufread::{GzDecoder, GzEncoder};
use flate2::Compression;

use crate::db::serialization::{StorageSerde, StorageSerdeError};

// TODO: consider changing the compression hyperparameters: compression level and algorithm.

/// Returns the compressed data in a vector.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = GzEncoder::new(data, Compression::default());
    let mut compressed_data = Vec::new();
    encoder.read_to_end(&mut compressed_data)?;
    Ok(compressed_data)
}

/// Compress object and returns his compression in a vector.
pub fn compress_object(object: &impl StorageSerde) -> Result<Vec<u8>, StorageSerdeError> {
    let mut buf = Vec::new();
    object.serialize_into(&mut buf)?;
    Ok(compress(buf.as_slice())?)
}

/// Decompress data and returns the uncompressed bytes in a vector.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = GzDecoder::new(data);
    let mut uncompressed = Vec::new();
    decoder.read_to_end(&mut uncompressed)?;
    Ok(uncompressed)
}

/// Decompress a vector directly from a reader.
pub fn decompress_from_reader(bytes: &mut impl std::io::Read) -> Option<Vec<u8>> {
    let compressed_data = Vec::<u8>::deserialize_from(bytes)?;
    decompress(compressed_data.as_slice()).ok()
}

#[cfg(test)]
mod compression_utils_test {
    use rand::distributions::Uniform;
    use rand::Rng;
    use starknet_api::deprecated_contract_class::Program;
    use test_utils::read_json_file;

    use super::{compress, compress_object, decompress, decompress_from_reader};
    use crate::db::serialization::StorageSerde;

    #[test]
    fn bytes_compression() {
        let length = rand::thread_rng().gen_range(0..10000);
        let range = Uniform::from(0..u8::MAX);
        let values: Vec<u8> = rand::thread_rng().sample_iter(&range).take(length).collect();
        let x = decompress(compress(values.as_slice()).unwrap().as_slice()).unwrap();
        assert_eq!(values, x);
    }

    #[test]
    fn object_compression() {
        let program_json = read_json_file("program.json");
        let program = serde_json::from_value::<Program>(program_json).unwrap();
        let compressed = compress_object(&program).unwrap();
        let mut buf = Vec::new();
        compressed.serialize_into(&mut buf).unwrap();
        let decompressed = decompress_from_reader(&mut buf.as_slice()).unwrap();
        let restored_program = Program::deserialize_from(&mut decompressed.as_slice()).unwrap();
        assert_eq!(program, restored_program);
    }
}
