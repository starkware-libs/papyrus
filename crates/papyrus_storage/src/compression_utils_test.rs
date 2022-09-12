use starknet_api::Program;

use super::{decode, encode};
use crate::test_utils::read_resource_file;

#[test]
fn encode_decode_program() {
    let program: Program = serde_json::from_str(&read_resource_file("program.json"))
        .expect("Failed to serde program resource file.");

    let encoded = encode(program.clone()).unwrap();
    let mut buff = Vec::new();
    let decoded = decode(encoded, &mut buff).unwrap();
    assert_eq!(program, decoded);
}
