use starknet_api::transaction::InvokeTransaction;
use test_utils::read_json_file;
use tracing::debug;

use crate::db::serialization::{StorageSerde, StorageSerdeEx};
use crate::state::data::ThinStateDiff;

#[test]
fn storage_serde_ex_small_thin_state_diff() {
    let _ = simple_logger::init_with_env();

    let diff_json = read_json_file("small_thin_state_diff.json");
    let diff: ThinStateDiff = serde_json::from_value(diff_json).unwrap();

    let mut buff = Vec::new();
    diff.serialize_into(&mut buff).unwrap();
    let len_without_compression = buff.len();

    let serialized = diff.serialize().unwrap();
    let len_after_serialization = serialized.len();
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinStateDiff::deserialize(&mut bytes.as_ref());
    assert_eq!(diff, deserialized.unwrap());

    debug!("The length of the serialized data: {:?}", len_after_serialization);
    debug!("The length of the serialized data without compression: {:?}", len_without_compression);
}

#[test]
fn storage_serde_ex_thin_state_diff() {
    let _ = simple_logger::init_with_env();

    let diff_json = read_json_file("thin_state_diff.json");
    let diff: ThinStateDiff = serde_json::from_value(diff_json).unwrap();

    let mut buff = Vec::new();
    diff.serialize_into(&mut buff).unwrap();
    let len_without_compression = buff.len();

    let serialized = diff.serialize().unwrap();
    let len_after_serialization = serialized.len();
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinStateDiff::deserialize(&mut bytes.as_ref());
    assert_eq!(diff, deserialized.unwrap());

    debug!("The length of the serialized data: {:?}", len_after_serialization);
    debug!("The length of the serialized data without compression: {:?}", len_without_compression);
    assert!(len_after_serialization < len_without_compression);
}

#[test]
fn storage_serde_ex_small_invoke() {
    let _ = simple_logger::init_with_env();

    let tx_json = read_json_file("small_invoke_transaction.json");
    let tx: InvokeTransaction = serde_json::from_value(tx_json).unwrap();

    let mut buff = Vec::new();
    tx.serialize_into(&mut buff).unwrap();
    let len_without_compression = buff.len();

    let serialized = tx.serialize().unwrap();
    let len_after_serialization = serialized.len();
    let bytes = serialized.into_boxed_slice();
    let deserialized = InvokeTransaction::deserialize(&mut bytes.as_ref());
    assert_eq!(tx, deserialized.unwrap());

    debug!("The length of the serialized data: {:?}", len_after_serialization);
    debug!("The length of the serialized data without compression: {:?}", len_without_compression);
}

#[test]
fn storage_serde_ex_invoke() {
    let _ = simple_logger::init_with_env();

    let tx_json = read_json_file("invoke_transaction.json");
    let tx: InvokeTransaction = serde_json::from_value(tx_json).unwrap();

    let mut buff = Vec::new();
    tx.serialize_into(&mut buff).unwrap();
    let len_without_compression = buff.len();

    let serialized = tx.serialize().unwrap();
    let len_after_serialization = serialized.len();
    let bytes = serialized.into_boxed_slice();
    let deserialized = InvokeTransaction::deserialize(&mut bytes.as_ref());
    assert_eq!(tx, deserialized.unwrap());

    debug!("The length of the serialized data: {:?}", len_after_serialization);
    debug!("The length of the serialized data without compression: {:?}", len_without_compression);
    assert!(len_after_serialization < len_without_compression);
}
