use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

pub fn run_serde_test<T: Serialize + DeserializeOwned + PartialEq + Debug>(val: &T) {
    assert_eq!(
        *val,
        serde_json::from_str::<T>(&serde_json::to_string(val).unwrap()).unwrap()
    )
}
