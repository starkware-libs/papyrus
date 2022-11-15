use starknet_api::state::Program;

use crate::compression_utils::GzEncoded;
use crate::test_utils::read_json_file;

#[test]
fn gzip_encode_decode_contract_program() -> Result<(), anyhow::Error> {
    let program_json = read_json_file("program.json")?;
    let program: Program = serde_json::from_value(program_json)?;
    let program_as_bytes = serde_json::to_vec(&program)?;
    let len_before_compression = program_as_bytes.len();

    let encoded = GzEncoded::encode(program.clone())?;
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff)?;
    assert_eq!(program, decoded);
    assert!(encoded.0.len() < len_before_compression);

    Ok(())
}
