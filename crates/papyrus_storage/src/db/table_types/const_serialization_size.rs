#[cfg(test)]
#[path = "const_serialization_size_test.rs"]
mod const_serialization_size_test;

use starknet_api::block::BlockNumber;

use crate::body::events::EventIndex;
use crate::db::serialization::{NoVersionValueWrapper, StorageSerde, VersionZeroWrapper};
#[cfg(test)]
use crate::db::table_types::const_serialization_size::const_serialization_size_test::{
    const_serialization_size_test,
    wrappers_const_size_serialization_test,
};
use crate::serializers::ValuePlaceHolder;

// A type that has a known serialization size.
pub(crate) trait ConstSerializationSize {
    const SIZE: usize;
}

// TODO(dvir): consider make this automatic like the auto_storage_serde macro.
impl_const_serialization_size! {
    BlockNumber: 4;
    u32: 4;
    EventIndex: 15;
}

impl ConstSerializationSize for ValuePlaceHolder {
    const SIZE: usize = 0;
}

// TODO(dvir): Make the creation of the tests automatic and do not repeat for each case. This will
// include to create a function name from the type name and use it in the test name.
// Consider using the same way for the auto_storage_serde macro.
#[allow(unused_macro_rules)]
macro_rules! impl_const_serialization_size{
    () => {};
    // Tuples - two elements.
    (($ty0:ty, $ty1:ty): $size:expr; $($rest:tt)*)=>{
        impl ConstSerializationSize for ($ty0, $ty1){
            const SIZE: usize = $size;
        }

        #[cfg(test)]
        paste::paste! {
            #[test]
            fn [<"const_serialization_size_test_tuple" _$ty0:snake _$ty1:snake>]() {
                const_serialization_size_test::<($ty0, $ty1)>();
            }

            #[test]
            fn [<"wrappers_const_serialization_size_test_tuple" _$ty0:snake _$ty1:snake>]() {
                wrappers_const_size_serialization_test::<($ty0, $ty1)>();
            }
        }

        impl_const_serialization_size!($($rest)*);
    };
    // Tuples - three elements.
    (($ty0:ty, $ty1:ty, $ty2:ty): $size:expr; $($rest:tt)*)=>{
        impl ConstSerializationSize for ($ty0, $ty1, $ty2){
            const SIZE: usize = $size;
        }

        #[cfg(test)]
        paste::paste! {
            #[test]
            fn [<"const_serialization_size_test_tuple" _$ty0:snake _$ty1:snake _$ty2:snake>]() {
                const_serialization_size_test::<($ty0, $ty1, $ty2)>();
            }

            #[test]
            fn [<"wrappers_const_serialization_size_test_tuple" _$ty0:snake _$ty1:snake _$ty2:snake>]() {
                wrappers_const_size_serialization_test::<($ty0, $ty1, $ty2)>();
            }
        }

        impl_const_serialization_size!($($rest)*);
    };
    // Single type.
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
    };
}
pub(crate) use impl_const_serialization_size;

// Implement the trait for the value wrappers.
impl<T: StorageSerde + ConstSerializationSize> ConstSerializationSize for NoVersionValueWrapper<T> {
    const SIZE: usize = T::SIZE;
}

impl<T: StorageSerde + ConstSerializationSize> ConstSerializationSize for VersionZeroWrapper<T> {
    const SIZE: usize = T::SIZE + 1;
}
