#[cfg(test)]
#[path = "const_serialization_size_test.rs"]
mod const_serialization_size_test;

use starknet_api::block::BlockNumber;

use crate::db::serialization::{NoVersionValueWrapper, StorageSerde, VersionZeroWrapper};
#[cfg(test)]
use crate::db::table_types::const_serialization_size::const_serialization_size_test::{
    const_serialization_size_test,
    wrappers_const_size_serialization_test,
};

// A type that has a known serialization size.
pub(crate) trait ConstSerializationSize {
    const SIZE: usize;
}

// TODO(dvir): consider make this automatic like the auto_storage_serde macro.
impl_const_serialization_size! {
    BlockNumber: 8;
    u32: 4;
}

macro_rules! impl_const_serialization_size{
    () => {};
    ($type:ty: $size:expr; $($rest:tt)*) => {
        impl ConstSerializationSize for $type{
            const SIZE: usize = $size;
        }

        #[cfg(test)]
        paste::paste! {
            #[test]
            fn [<"const_serialization_size_test" _$type:snake>]() {
                const_serialization_size_test::<$type>();
            }

            #[test]
            fn [<"wrappers_const_serialization_size_test" _$type:snake>]() {
                wrappers_const_size_serialization_test::<$type>();
            }
    }

        impl_const_serialization_size!($($rest)*);
    }
}
use impl_const_serialization_size;

// Implement the trait for the value wrappers.
impl<T: StorageSerde + ConstSerializationSize> ConstSerializationSize for NoVersionValueWrapper<T> {
    const SIZE: usize = T::SIZE;
}

impl<T: StorageSerde + ConstSerializationSize> ConstSerializationSize for VersionZeroWrapper<T> {
    const SIZE: usize = T::SIZE + 1;
}
