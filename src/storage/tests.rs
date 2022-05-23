use std::sync::{Arc, Mutex};

use crate::{
    starknet::BlockNumber,
    storage::{DataStore, StarknetStorageReader, StarknetStorageWriter, StorageError},
};

struct MockDataStore {
    latest_block_num: BlockNumber,
}

struct DataStoreHandle {
    inner: Arc<Mutex<MockDataStore>>,
}

fn create_mock_store() -> DataStoreHandle {
    DataStoreHandle {
        inner: Arc::new(Mutex::new(MockDataStore {
            latest_block_num: BlockNumber(0),
        })),
    }
}

struct MockWriter {
    mock_store: Arc<Mutex<MockDataStore>>,
}

struct MockReader {
    mock_store: Arc<Mutex<MockDataStore>>,
}

impl StarknetStorageReader for MockReader {
    fn get_latest_block_number(&self) -> BlockNumber {
        self.mock_store.lock().unwrap().latest_block_num //should be try_lock?
    }
}

impl StarknetStorageWriter for MockWriter {
    fn set_latest_block_number(&mut self, n: BlockNumber) {
        self.mock_store.lock().unwrap().latest_block_num = n;
    }
}

impl DataStore for DataStoreHandle {
    type R = MockReader;
    type W = MockWriter;

    fn get_state_read_access(&self) -> Result<MockReader, StorageError> {
        Ok(MockReader {
            mock_store: self.inner.clone(),
        })
    }

    fn get_state_write_access(&self) -> Result<MockWriter, StorageError> {
        Ok(MockWriter {
            mock_store: self.inner.clone(),
        })
    }
}

#[test]
fn test_add_block_number() {
    let data_store_handle = create_mock_store();
    let expected = BlockNumber(5);

    match data_store_handle.get_state_write_access() {
        Err(_e) => panic!("Could not get write access"),
        Ok(mut sw) => {
            sw.set_latest_block_number(expected);

            match data_store_handle.get_state_read_access() {
                Err(_e) => panic!("Could not get read access"),
                Ok(sr) => {
                    let res = sr.get_latest_block_number();
                    assert_eq!(res, BlockNumber(5));
                }
            }
        }
    }
}
