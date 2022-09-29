use starknet_api::Program;

use super::GzEncoded;
use crate::db::serialization::StorageSerdeEx;
use crate::test_utils::{get_test_block, read_resource_file};

#[test]
fn gzip_encode_decode_contract_program() -> Result<(), anyhow::Error> {
    let program_json = read_resource_file("program.json")?;
    let program: Program = serde_json::from_str(&program_json)?;
    let program_as_bytes = serde_json::to_vec(&program)?;
    let len_before_compression = program_as_bytes.len();

    // Note that we cannot encode the program directly, since bincode does not work for the
    // serde_json::Value fields.
    // TODO(anatg): Fix this when the StorageSerde implementation for program doesn't call bincode
    // anymore.
    let encoded = GzEncoded::encode(program_as_bytes)?;
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff)?;
    let decoded_program = serde_json::from_slice::<Program>(decoded.as_slice())?;
    assert_eq!(program, decoded_program);
    assert!(encoded.0.len() < len_before_compression);

    Ok(())
}

#[test]
fn gzip_encode_decode_block() -> Result<(), anyhow::Error> {
    let block = get_test_block(2);
    let block_as_bytes = block.serialize();
    let len_before_compression = block_as_bytes.len();

    let encoded = GzEncoded::encode(block.clone())?;
    let mut buff = Vec::new();
    let decoded = encoded.decode(&mut buff)?;
    assert_eq!(block, decoded);
    assert!(encoded.0.len() < len_before_compression);

    Ok(())
}
