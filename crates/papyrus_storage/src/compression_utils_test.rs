use starknet_api::Program;

use super::CompressedObject;
use crate::test_utils::read_resource_file;

#[test]
fn encode_decode_program() {
    let program_json = read_resource_file("program.json");
    let program: Program =
        serde_json::from_str(&program_json).expect("Failed to serde program resource file.");

    let encoded = CompressedObject::encode(program.clone()).unwrap();
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff).unwrap();
    assert_eq!(program, decoded);
    assert!(encoded.0.len() < program_json.len());
}
