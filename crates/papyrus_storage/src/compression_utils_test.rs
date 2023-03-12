use starknet_api::state::Program;
use test_utils::read_json_file;
use tracing::debug;

use crate::compression_utils::GzEncoded;
use crate::db::serialization::StorageSerde;
use crate::state::data::{IndexedDeclaredContract, ThinStateDiff};

#[test]
fn gzip_encode_decode_contract_program() {
    let _ = simple_logger::init_with_env();

    let program_json = read_json_file("program.json");
    let program: Program = serde_json::from_value(program_json).unwrap();
    let mut buff = Vec::new();
    program.serialize_into(&mut buff).unwrap();
    let len_before_compression = buff.len();

    let encoded = GzEncoded::encode(&program).unwrap();
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff).unwrap();
    assert_eq!(program, decoded);

    let len_after_compression = encoded.0.len();
    debug!("The length of the serialized data after compression: {:?}", len_after_compression);
    debug!("The length of the serialized data without compression: {:?}", len_before_compression);
    assert!(len_after_compression < len_before_compression);
}

#[test]
fn gzip_encode_decode_indexed_declared_contract() {
    let _ = simple_logger::init_with_env();

    let contract_json = read_json_file("indexed_declared_contract.json");
    let contract: IndexedDeclaredContract = serde_json::from_value(contract_json).unwrap();
    let mut buff = Vec::new();
    contract.serialize_into(&mut buff).unwrap();
    let len_before_compression = buff.len();

    let encoded = GzEncoded::encode(&contract).unwrap();
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff).unwrap();
    assert_eq!(contract, decoded);

    let len_after_compression = encoded.0.len();
    debug!("The length of the serialized data after compression: {:?}", len_after_compression);
    debug!("The length of the serialized data without compression: {:?}", len_before_compression);
    assert!(len_after_compression < len_before_compression);
}

#[test]
fn gzip_encode_decode_thin_state_diff() {
    let _ = simple_logger::init_with_env();

    let diff_json = read_json_file("thin_state_diff.json");
    let diff: ThinStateDiff = serde_json::from_value(diff_json).unwrap();
    let mut buff = Vec::new();
    diff.serialize_into(&mut buff).unwrap();
    let len_before_compression = buff.len();

    let encoded = GzEncoded::encode(&diff).unwrap();
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff).unwrap();
    assert_eq!(diff, decoded);

    let len_after_compression = encoded.0.len();
    debug!("The length of the serialized data after compression: {:?}", len_after_compression);
    debug!("The length of the serialized data without compression: {:?}", len_before_compression);
    assert!(len_after_compression < len_before_compression);
}
