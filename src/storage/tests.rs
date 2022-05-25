// use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{
    starknet::BlockNumber,
    storage::{create_store_access, DataStore, StarknetStorageReader, StarknetStorageWriter},
};

#[test]
fn test_add_block_number() {
    //we use unwrap throughout this functio since it's
    //a test function using an internal mock implementation.

    let data_store_handle = create_store_access().unwrap();
    let (reader, mut writer) = data_store_handle.get_access().unwrap();
    let expected = BlockNumber(5);

    writer.set_latest_block_number(expected).unwrap();

    let res = reader.get_latest_block_number();
    assert_eq!(res.unwrap(), BlockNumber(5));
}
