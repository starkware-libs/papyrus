use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::state::{ContractClass, StateDiff, StorageEntry, StorageKey};
use starknet_api::{patky, shash};

use crate::{StorageSerde, ThinStateDiff};

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test();
}

// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() {
        let item = T::get_test_instance();
        let mut serialized: Vec<u8> = Vec::new();
        item.serialize_into(&mut serialized).unwrap();
        let bytes = serialized.into_boxed_slice();
        let deserialized = T::deserialize_from(&mut bytes.as_ref());
        assert_eq!(item, deserialized.unwrap());
    }
}

pub trait GetTestInstance: Sized {
    fn get_test_instance() -> Self;
}

// Tests all types that implement the [`StorageSerde`] trait
// via the [`auto_storage_serde`] macro.
macro_rules! auto_storage_serde_test {
    ($name:ident, $($full_exp:tt)*) => {
        impl_get_test_instance!($($full_exp)*);
        create_test!($name);
    };
    (($ty0:ty, $ty1:ty)) => {
        impl_get_test_instance!(($ty0, $ty1));
        create_test!(($ty0, $ty1));
    };
    (($ty0:ty, $ty1:ty, $ty2:ty)) => {
        impl_get_test_instance!(($ty0, $ty1, $ty2));
        create_test!(($ty0, $ty1, $ty2));
    };
}
pub(crate) use auto_storage_serde_test;

// Creates tests that call the [`storage_serde_test`] function.
macro_rules! create_test {
    ($name:ident) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$name:snake>]() {
                $name::storage_serde_test()
            }
        }
    };
    (($ty0:ty, $ty1:ty)) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$ty0:snake _$ty1:snake>]() {
                <($ty0, $ty1)>::storage_serde_test()
            }
        }
    };
    (($ty0:ty, $ty1:ty, $ty2:ty)) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$ty0:snake _$ty1:snake _$ty2:snake>]() {
                <($ty0, $ty1, $ty2)>::storage_serde_test()
            }
        }
    };
}
pub(crate) use create_test;

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for primitive types.
////////////////////////////////////////////////////////////////////////
impl GetTestInstance for serde_json::Value {
    fn get_test_instance() -> Self {
        serde_json::from_str(r#""0x1""#).unwrap()
    }
}
impl GetTestInstance for String {
    fn get_test_instance() -> Self {
        "a".to_string()
    }
}
impl<T: GetTestInstance> GetTestInstance for Option<T> {
    fn get_test_instance() -> Self {
        Some(T::get_test_instance())
    }
}
impl<T: GetTestInstance> GetTestInstance for Vec<T> {
    fn get_test_instance() -> Self {
        vec![T::get_test_instance()]
    }
}
impl<K: GetTestInstance + Eq + Hash, V: GetTestInstance> GetTestInstance for HashMap<K, V> {
    fn get_test_instance() -> Self {
        let mut res = HashMap::with_capacity(1);
        let k = K::get_test_instance();
        let v = V::get_test_instance();
        res.insert(k, v);
        res
    }
}
impl<T: GetTestInstance + Default + Copy, const N: usize> GetTestInstance for [T; N] {
    fn get_test_instance() -> Self {
        [T::get_test_instance(); N]
    }
}

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for the types that the
// [`auto_storage_serde`] macro is called with.
////////////////////////////////////////////////////////////////////////
macro_rules! impl_get_test_instance {
    // Tuple structs (no names associated with fields) - one field.
    (struct $name:ident($ty:ty)) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self(<$ty>::get_test_instance())
            }
        }
    };
    // Tuple structs (no names associated with fields) - two fields.
    (struct $name:ident($ty0:ty, $ty1:ty)) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self(<$ty0>::get_test_instance(), <$ty1>::get_test_instance())
            }
        }
    };
    // Structs with public fields.
    (struct $name:ident { $(pub $field:ident : $ty:ty ,)* }) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self {
                    $(
                        $field: <$ty>::get_test_instance(),
                    )*
                }
            }
        }
    };
    // Tuples - two elements.
    (($ty0:ty, $ty1:ty)) => {
        impl GetTestInstance for ($ty0, $ty1) {
            fn get_test_instance() -> Self {
                (
                    <$ty0>::get_test_instance(),
                    <$ty1>::get_test_instance(),
                )
            }
        }
    };
    // Tuples - three elements.
    (($ty0:ty, $ty1:ty, $ty2:ty)) => {
        impl GetTestInstance for ($ty0, $ty1, $ty2) {
            fn get_test_instance() -> Self {
                (
                    <$ty0>::get_test_instance(),
                    <$ty1>::get_test_instance(),
                    <$ty2>::get_test_instance(),
                )
            }
        }
    };
    // Enums with no inner struct.
    (enum $name:ident { $variant:ident = $num:expr , $($rest:tt)* }) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self::$variant
            }
        }
    };
    // Enums with inner struct.
    (enum $name:ident { $variant:ident ($ty:ty) = $num:expr , $($rest:tt)* }) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self::$variant(<$ty>::get_test_instance())
            }
        }
    };
    // Binary.
    (bincode($name:ident)) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Self {
                Self::default()
            }
        }
    }
}
pub(crate) use impl_get_test_instance;

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for types not supported
// by the macro [`impl_get_test_instance`] and calls the [`create_test`]
// macro to create the tests for them.
////////////////////////////////////////////////////////////////////////
impl GetTestInstance for ThinStateDiff {
    fn get_test_instance() -> Self {
        let state_diff = StateDiff::new(
            vec![(ContractAddress::get_test_instance(), ClassHash::get_test_instance())],
            vec![(ContractAddress::get_test_instance(), vec![StorageEntry::get_test_instance()])],
            vec![(ClassHash::get_test_instance(), ContractClass::get_test_instance())],
            vec![(ContractAddress::get_test_instance(), Nonce::get_test_instance())],
        )
        .unwrap();
        ThinStateDiff::from(state_diff)
    }
}
create_test!(ThinStateDiff);

impl GetTestInstance for StarkHash {
    fn get_test_instance() -> Self {
        shash!("0x1")
    }
}
create_test!(StarkHash);

impl GetTestInstance for ContractAddress {
    fn get_test_instance() -> Self {
        Self(patky!("0x1"))
    }
}
create_test!(ContractAddress);

impl GetTestInstance for StorageKey {
    fn get_test_instance() -> Self {
        Self(patky!("0x1"))
    }
}
create_test!(StorageKey);
