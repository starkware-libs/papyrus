use starknet_api::Program;

use super::{Base64Encoded, GzEncoded};
use crate::test_utils::read_resource_file;

#[test]
fn gzip_encode_decode_program() -> Result<(), anyhow::Error> {
    let program_json = read_resource_file("program.json")?;
    let program: Program = serde_json::from_str(&program_json)?;

    let encoded = GzEncoded::encode(program.clone())?;
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff)?;
    assert_eq!(program, decoded);
    assert!(encoded.0.len() < program_json.len());

    Ok(())
}

#[test]
fn base64_encode_decode_program_json() -> Result<(), anyhow::Error> {
    let program_json = read_resource_file("program.json")?;

    let encoded = Base64Encoded::encode(program_json.clone())?;
    let decoded = encoded.decode()?;
    assert_eq!(program_json.as_bytes(), decoded.0.as_slice());

    Ok(())
}
