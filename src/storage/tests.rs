use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{
    starknet::BlockNumber,
    storage::{DataStore, StarknetStorageReader, StarknetStorageWriter, StorageError},
};

impl From<PoisonError<MutexGuard<'_, MockDataStore>>> for StorageError {
    fn from(_: PoisonError<MutexGuard<MockDataStore>>) -> Self {
        StorageError {}
    }
}
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
    fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        Ok(self.mock_store.lock()?.latest_block_num)
    }
}

impl StarknetStorageWriter for MockWriter {
    fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError> {
        self.mock_store.lock()?.latest_block_num = n;
        Ok(())
    }
}

impl DataStoreHandle {
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

impl DataStore for DataStoreHandle {
    type R = MockReader;
    type W = MockWriter;

    fn get_access(&self) -> Result<(MockReader, MockWriter), StorageError> {
        Ok((
            self.get_state_read_access()?,
            self.get_state_write_access()?,
        ))
    }
}

#[test]
fn test_add_block_number() {
    //we use unwrap throughout this functio since it's
    //a test function using an internal mock implementation.

    let data_store_handle = create_mock_store();
    let (reader, mut writer) = data_store_handle.get_access().unwrap();
    let expected = BlockNumber(5);

    // let mut writer = data_store_handle.get_state_write_access().unwrap();
    writer.set_latest_block_number(expected).unwrap();

    // let reader = data_store_handle.get_state_read_access().unwrap();
    let res = reader.get_latest_block_number();
    assert_eq!(res.unwrap(), BlockNumber(5));
}
