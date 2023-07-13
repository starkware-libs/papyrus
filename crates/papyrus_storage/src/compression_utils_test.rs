use papyrus_test_utils::read_json_file;
use pretty_assertions::assert_eq;
use starknet_api::deprecated_contract_class::Program;

use super::{compress, decompress, decompress_from_reader, serialize_and_compress};
use crate::db::serialization::StorageSerde;

#[test]
fn bytes_compression() {
    let bytes = vec![30, 5, 23, 12, 47];
    let x = decompress(compress(bytes.as_slice()).unwrap().as_slice()).unwrap();
    assert_eq!(bytes, x);
}

#[test]
fn object_compression() {
    let program_json = read_json_file("program.json");
    let program = serde_json::from_value::<Program>(program_json).unwrap();
    let compressed = serialize_and_compress(&program).unwrap();
    let mut buf = Vec::new();
    compressed.serialize_into(&mut buf).unwrap();
    let decompressed = decompress_from_reader(&mut buf.as_slice()).unwrap();
    let restored_program = Program::deserialize_from(&mut decompressed.as_slice()).unwrap();
    assert_eq!(program, restored_program);
}
