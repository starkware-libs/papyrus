use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use starknet_api::{
    shash, ContractAddress, ContractNonce, DeclaredContract, DeployedContract, StarkHash,
    StateDiff, StorageDiff, StorageEntry, StorageKey,
};

use crate::{StorageSerde, ThinStateDiff};

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test() -> Result<(), anyhow::Error>;
}

// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() -> Result<(), anyhow::Error> {
        let item = T::get_test_instance()?;
        let mut serialized: Vec<u8> = Vec::new();
        item.serialize_into(&mut serialized)?;
        let bytes = serialized.into_boxed_slice();
        let deserialized = T::deserialize_from(&mut bytes.as_ref());
        assert_eq!(item, deserialized.unwrap());

        Ok(())
    }
}

pub trait GetTestInstance: Sized {
    fn get_test_instance() -> Result<Self, anyhow::Error>;
}

// Tests all types that implement the [`StorageSerde`] trait
// via the [`auto_storage_serde`] macro.
macro_rules! auto_storage_serde_test {
    ($name:ident, $($rest:tt)*) => {
        impl_get_test_instance!($($rest)*);
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
            fn [<"storage_serde_test" _$name:snake>]() -> Result<(), anyhow::Error> {
                $name::storage_serde_test()
            }
        }
    };
    (($ty0:ty, $ty1:ty)) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$ty0:snake _$ty1:snake>]() -> Result<(), anyhow::Error> {
                <($ty0, $ty1)>::storage_serde_test()
            }
        }
    };
    (($ty0:ty, $ty1:ty, $ty2:ty)) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$ty0:snake _$ty1:snake _$ty2:snake>]() -> Result<(), anyhow::Error> {
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
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok(serde_json::from_str(r#""0x1""#)?)
    }
}
impl GetTestInstance for String {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok("a".to_string())
    }
}
impl<T: GetTestInstance> GetTestInstance for Option<T> {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok(Some(T::get_test_instance()?))
    }
}
impl<T: GetTestInstance> GetTestInstance for Vec<T> {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok(vec![T::get_test_instance()?])
    }
}
impl<K: GetTestInstance + Eq + Hash, V: GetTestInstance> GetTestInstance for HashMap<K, V> {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        let mut res = HashMap::with_capacity(1);
        let k = K::get_test_instance()?;
        let v = V::get_test_instance()?;
        res.insert(k, v);
        Ok(res)
    }
}
impl<T: GetTestInstance + Default + Copy, const N: usize> GetTestInstance for [T; N] {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok([T::get_test_instance()?; N])
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
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok(Self(<$ty>::get_test_instance()?))
            }
        }
    };
    // Tuple structs (no names associated with fields) - two fields.
    (struct $name:ident($ty0:ty, $ty1:ty)) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok(Self(<$ty0>::get_test_instance()?, <$ty1>::get_test_instance()?))
            }
        }
    };
    // Structs with public fields.
    (struct $name:ident { $(pub $field:ident : $ty:ty ,)* }) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok(Self {
                    $(
                        $field: <$ty>::get_test_instance()?,
                    )*
                })
            }
        }
    };
    // Tuples - two elements.
    (($ty0:ty, $ty1:ty)) => {
        impl GetTestInstance for ($ty0, $ty1) {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok((
                    <$ty0>::get_test_instance()?,
                    <$ty1>::get_test_instance()?,
                ))
            }
        }
    };
    // Tuples - three elements.
    (($ty0:ty, $ty1:ty, $ty2:ty)) => {
        impl GetTestInstance for ($ty0, $ty1, $ty2) {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok((
                    <$ty0>::get_test_instance()?,
                    <$ty1>::get_test_instance()?,
                    <$ty2>::get_test_instance()?,
                ))
            }
        }
    };
    // Enums with no inner struct.
    (enum $name:ident { $variant:ident = $num:expr , $($rest:tt)* }) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok(Self::$variant)
            }
        }
    };
    // Enums with inner struct.
    (enum $name:ident { $variant:ident ($ty:ty) = $num:expr , $($rest:tt)* }) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok(Self::$variant(<$ty>::get_test_instance()?))
            }
        }
    };
    // Binary.
    (bincode($name:ident)) => {
        impl GetTestInstance for $name {
            fn get_test_instance() -> Result<Self, anyhow::Error> {
                Ok(Self::default())
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
    fn get_test_instance() -> Result<ThinStateDiff, anyhow::Error> {
        let state_diff = StateDiff::new(
            vec![DeployedContract::get_test_instance()?],
            vec![StorageDiff::get_test_instance()?],
            vec![DeclaredContract::get_test_instance()?],
            vec![ContractNonce::get_test_instance()?],
        )?;
        Ok(ThinStateDiff::from(state_diff))
    }
}
create_test!(ThinStateDiff);

impl GetTestInstance for StorageDiff {
    fn get_test_instance() -> Result<StorageDiff, anyhow::Error> {
        Ok(Self::new(
            ContractAddress::get_test_instance()?,
            vec![StorageEntry {
                key: StorageKey::get_test_instance()?,
                value: StarkHash::get_test_instance()?,
            }],
        )?)
    }
}
create_test!(StorageDiff);

impl GetTestInstance for StarkHash {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok(shash!("0x1"))
    }
}
create_test!(StarkHash);

impl GetTestInstance for ContractAddress {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok(Self::try_from(shash!("0x1"))?)
    }
}
create_test!(ContractAddress);

impl GetTestInstance for StorageKey {
    fn get_test_instance() -> Result<Self, anyhow::Error> {
        Ok(Self::try_from(shash!("0x1"))?)
    }
}
create_test!(StorageKey);
