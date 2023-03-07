use std::fmt::Debug;

use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_api::state::{Program, StorageKey};
use test_utils::{get_rng, GetTestInstance};

use crate::db::serialization::StorageSerde;

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test();
}

// TODO(anatg): Consider change the test so it tests StorageSerdeEx instead of StorageSerde.
// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() {
        let mut rng = get_rng(None);
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
create_storage_serde_test!(Program);

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
