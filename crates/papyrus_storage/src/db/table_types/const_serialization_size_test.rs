use std::fmt::Debug;

use starknet_api::block::BlockNumber;
use test_utils::{get_rng, GetTestInstance};

use super::ConstSerializationSize;
use crate::db::serialization::{
    NoVersionValueWrapper,
    StorageSerde,
    StorageSerdeEx,
    VersionZeroWrapper,
};
use crate::ValueSerde;

pub(super) fn const_serialization_size_test<
    T: ConstSerializationSize + GetTestInstance + StorageSerde,
>() {
    let value = T::get_test_instance(&mut get_rng());
    let serialize = value.serialize().unwrap();
    assert_eq!(serialize.len(), T::SIZE);
}

pub(super) fn wrappers_const_size_serialization_test<
    T: ConstSerializationSize + GetTestInstance + StorageSerde + Debug,
>() {
    let value = T::get_test_instance(&mut get_rng());
    assert_eq!(
        NoVersionValueWrapper::<T>::serialize(&value).unwrap().len(),
        NoVersionValueWrapper::<T>::SIZE
    );
    assert_eq!(
        VersionZeroWrapper::<T>::serialize(&value).unwrap().len(),
        VersionZeroWrapper::<T>::SIZE
    );
}

// Additional tests for special cases.
#[test]
fn additional_const_serialization_test() {
    check_default_serialization_size::<u32>();
    check_default_serialization_size::<BlockNumber>();
}

// TODO(dvir): consider add the default test case to the macro.
fn check_default_serialization_size<T: ConstSerializationSize + Default + StorageSerde>() {
    let default = T::default();
    let serialize = default.serialize().unwrap();
    assert_eq!(serialize.len(), T::SIZE);
}
