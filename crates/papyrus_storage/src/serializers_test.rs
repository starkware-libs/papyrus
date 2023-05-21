use core::fmt::Debug;
use std::env;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use tempfile::NamedTempFile;
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
create_storage_serde_test!(StarkHash);
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

#[test]
fn casm_serde_regression_test() {
    // TODO(yair): Keep compiled class instances spanning all of the possible types.
    const NUM_TEST_OBJECTS: u8 = 1;
    for i in 0..NUM_TEST_OBJECTS {
        // Get the casm and its serialization.
        let casm_json = read_json_file(format!("{i}_compiled_class.json").as_str());
        let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
        let mut expected_serialization = Vec::new();

        let serialization_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
            .join(format!("resources/{i}_serialization.bin"));
        BufReader::new(File::open(serialization_path.clone()).unwrap())
            .read_to_end(&mut expected_serialization)
            .unwrap();

        // Check that the serialization didn't change.
        let mut serialized = Vec::new();
        expected_casm.serialize_into(&mut serialized).unwrap();

        if expected_serialization != serialized {
            let (mut temp_file, path) = NamedTempFile::new().unwrap().keep().unwrap();
            temp_file.write_all(serialized.as_slice()).unwrap();

            let casm_json_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
                .join(format!("resources/{i}_compiled_class.json"));

            panic!(
                "Storage serialization of {} changed. If this is intended, store the new \
                 serialization.\nThe computed serialization can be found in: {}\nStore the new \
                 serialization in: {}",
                casm_json_path.display(),
                path.display(),
                serialization_path.to_str().unwrap(),
            );
        }

        // Check that the deserialization returns the original object.
        let bytes = serialized.into_boxed_slice();
        let deserialized = CasmContractClass::deserialize_from(&mut bytes.as_ref()).unwrap();

        assert_eq!(expected_casm, deserialized);
    }
}
