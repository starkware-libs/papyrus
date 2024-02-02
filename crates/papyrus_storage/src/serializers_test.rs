use std::fmt::Debug;

use cairo_lang_casm::hints::CoreHintBase;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;
use test_utils::{get_rng, read_json_file, GetTestInstance};

use crate::db::serialization::StorageSerde;

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test();
}

// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() {
        let mut rng = get_rng();
        let item = T::get_test_instance(&mut rng);
        let mut serialized: Vec<u8> = Vec::new();
        item.serialize_into(&mut serialized).unwrap();
        let bytes = serialized.into_boxed_slice();
        let deserialized = T::deserialize_from(&mut bytes.as_ref());
        assert_eq!(item, deserialized.unwrap());
    }
}

// Tests all types that implement the [`StorageSerde`] trait
// via the [`auto_storage_serde`] macro.
macro_rules! create_storage_serde_test {
    ($name:ident) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$name:snake>]() {
                $name::storage_serde_test()
            }
        }
    };
}
pub(crate) use create_storage_serde_test;

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for types not supported
// by the macro [`impl_get_test_instance`] and calls the [`create_test`]
// macro to create the tests for them.
////////////////////////////////////////////////////////////////////////
create_storage_serde_test!(bool);
create_storage_serde_test!(ContractAddress);
create_storage_serde_test!(Felt);
create_storage_serde_test!(StorageKey);
create_storage_serde_test!(u8);
create_storage_serde_test!(usize);

#[test]
fn block_number_endianness() {
    let bn_255 = BlockNumber(255);
    let mut serialized: Vec<u8> = Vec::new();
    bn_255.serialize_into(&mut serialized).unwrap();
    let bytes_255 = serialized.into_boxed_slice();
    let deserialized = BlockNumber::deserialize_from(&mut bytes_255.as_ref());
    assert_eq!(bn_255, deserialized.unwrap());

    let bn_256 = BlockNumber(256);
    let mut serialized: Vec<u8> = Vec::new();
    bn_256.serialize_into(&mut serialized).unwrap();
    let bytes_256 = serialized.into_boxed_slice();
    let deserialized = BlockNumber::deserialize_from(&mut bytes_256.as_ref());
    assert_eq!(bn_256, deserialized.unwrap());

    assert!(bytes_255 < bytes_256);
}

// Make sure that the [`Hint`] schema is not modified. If it is, its encoding might change and a
// storage migration is needed.
#[test]
fn hint_modified() {
    // Only CoreHintBase is being used in programs (StarknetHint is for tests).
    let hint_schema = schemars::schema_for!(CoreHintBase);
    insta::assert_yaml_snapshot!(hint_schema);
}

// Tests the persistent encoding of the hints of an ERC20 contract.
// Each snapshot filename contains the hint's index in the origin casm file, so that a failure in
// the assertion of a file can lead to the hint that caused it.
#[test]
fn hints_regression() {
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file(
        "erc20_compiled_contract_class.json",
    ))
    .unwrap();
    for hint in casm.hints.iter() {
        let mut encoded_hint: Vec<u8> = Vec::new();
        hint.serialize_into(&mut encoded_hint)
            .unwrap_or_else(|_| panic!("Failed to serialize hint {hint:?}"));
        insta::assert_yaml_snapshot!(format!("hints_regression_hint_{}", hint.0), encoded_hint);
    }
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
