use std::sync::Arc;

use pretty_assertions::assert_eq;
use rand::Rng;
use tempfile::tempdir;
use tokio::sync::{Barrier, RwLock};

use super::*;
use crate::db::serialization::NoVersionValueWrapper;
use crate::test_utils::get_mmap_file_test_config;

#[test]
fn config_validation() {
    let mut config = get_mmap_file_test_config();
    config.max_size = config.growth_step - 1;
    assert!(config.validate().is_err());
    config.max_size = 1 << 27;
    assert!(config.validate().is_ok());

    config.growth_step = config.max_object_size - 1;
    assert!(config.validate().is_err());
    config.growth_step = 1 << 20;
    assert!(config.validate().is_ok());
}

#[test]
fn write_read() {
    let dir = tempdir().unwrap();
    let offset = 0;
    let (mut writer, reader) = open_file::<NoVersionValueWrapper<Vec<u8>>>(
        get_mmap_file_test_config(),
        dir.path().to_path_buf().join("test_write_read"),
        offset,
    )
    .unwrap();
    let data = vec![1, 2, 3];

    let location_in_file = writer.append(&data);
    let res_writer = writer.get(location_in_file).unwrap().unwrap();
    assert_eq!(res_writer, data);

    let res = reader.get(location_in_file).unwrap().unwrap();
    assert_eq!(res, data);

    dir.close().unwrap();
}

#[test]
fn concurrent_reads() {
    let dir = tempdir().unwrap();
    let offset = 0;
    let (mut writer, reader) = open_file::<NoVersionValueWrapper<Vec<u8>>>(
        get_mmap_file_test_config(),
        dir.path().to_path_buf().join("test_concurrent_reads"),
        offset,
    )
    .unwrap();
    let data = vec![1, 2, 3];

    let location_in_file = writer.append(&data);

    let num_threads = 50;
    let mut handles = vec![];

    for _ in 0..num_threads {
        let reader = reader.clone();
        let handle = std::thread::spawn(move || reader.get(location_in_file).unwrap());
        handles.push(handle);
    }

    for handle in handles {
        let res = handle.join().unwrap().unwrap();
        assert_eq!(res, data);
    }

    dir.close().unwrap();
}

#[test]
fn concurrent_reads_single_write() {
    let dir = tempdir().unwrap();
    let offset = 0;
    let (mut writer, reader) = open_file::<NoVersionValueWrapper<Vec<u8>>>(
        get_mmap_file_test_config(),
        dir.path().to_path_buf().join("test_concurrent_reads_single_write"),
        offset,
    )
    .unwrap();
    let first_data = vec![1, 2, 3];
    let second_data = vec![3, 2, 1];
    let first_location = writer.append(&first_data);
    writer.flush();
    let second_location =
        LocationInFile { offset: first_location.next_offset(), len: first_location.len };

    let n = 10;
    let barrier = Arc::new(std::sync::Barrier::new(n + 1));
    let mut handles = Vec::with_capacity(n);

    for _ in 0..n {
        let reader = reader.clone();
        let reader_barrier = barrier.clone();
        let first_data = first_data.clone();
        handles.push(std::thread::spawn(move || {
            assert_eq!(reader.get(first_location).unwrap().unwrap(), first_data);
            reader_barrier.wait();
            // readers wait for the writer to write the value.
            reader_barrier.wait();
            reader.get(second_location).unwrap()
        }));
    }
    // Writer waits for all readers to read the first value.
    barrier.wait();
    writer.append(&second_data);
    writer.flush();
    // Allow readers to proceed reading the second value.
    barrier.wait();

    for handle in handles {
        let res = handle.join().unwrap().unwrap();
        assert_eq!(res, second_data);
    }
}

#[test]
fn grow_file() {
    let data = vec![1, 2];
    let serialization_size = NoVersionValueWrapper::serialize(&data).unwrap().len();
    let dir = tempdir().unwrap();
    let config = MmapFileConfig {
        max_size: 10 * serialization_size,
        max_object_size: serialization_size, // 3 (len + data)
        growth_step: serialization_size + 1, // 4
    };

    let file_path = dir.path().to_path_buf().join("test_grow_file");
    let mut offset = 0;
    {
        let file =
            OpenOptions::new().read(true).write(true).create(true).open(file_path.clone()).unwrap();
        // file_size = 0, offset = 0
        assert_eq!(file.metadata().unwrap().len(), 0);

        let (mut writer, _) =
            open_file::<NoVersionValueWrapper<Vec<u8>>>(config.clone(), file_path.clone(), offset)
                .unwrap();
        // file_size = 4 (growth_step), offset = 0
        let mut file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, config.growth_step as u64);
        assert_eq!(offset, 0);

        offset += writer.append(&data).len;
        // file_size = 8 (2 * growth_step), offset = 3 (serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 2 * config.growth_step as u64);
        assert_eq!(offset, serialization_size);

        offset += writer.append(&data).len;
        // file_size = 12 (3 * growth_step), offset = 6 (2 * serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 3 * config.growth_step as u64);
        assert_eq!(offset, 2 * serialization_size);

        offset += writer.append(&data).len;
        // file_size = 12 (3 * growth_step), offset = 9 (3 * serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 3 * config.growth_step as u64);
        assert_eq!(offset, 3 * serialization_size);

        offset += writer.append(&data).len;
        // file_size = 16 (4 * growth_step), offset = 12 (4 * serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 4 * config.growth_step as u64);
        assert_eq!(offset, 4 * serialization_size);
    }

    let file =
        OpenOptions::new().read(true).write(true).create(true).open(file_path.clone()).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 4 * config.growth_step as u64);
    let _ = open_file::<NoVersionValueWrapper<Vec<u8>>>(config.clone(), file_path, offset).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 4 * config.growth_step as u64);

    dir.close().unwrap();
}

#[tokio::test]
async fn write_read_different_locations() {
    let dir = tempdir().unwrap();
    let offset = 0;
    let (mut writer, reader) = open_file(
        get_mmap_file_test_config(),
        dir.path().to_path_buf().join("test_write_read_different_locations"),
        offset,
    )
    .unwrap();
    let mut data = vec![0, 1];

    const ROUNDS: u8 = 10;
    const LEN: usize = 3;
    let n_readers_per_phase = 10;
    let barrier = Arc::new(Barrier::new(n_readers_per_phase + 1));
    let lock = Arc::new(RwLock::new(0));

    async fn reader_task(
        reader: FileHandler<NoVersionValueWrapper<Vec<u8>>, RO>,
        lock: Arc<RwLock<usize>>,
        barrier: Arc<Barrier>,
    ) {
        barrier.wait().await;
        let round: usize;
        {
            round = *lock.read().await;
        }
        let read_offset = 3 * rand::thread_rng().gen_range(0..round + 1);
        let read_location = LocationInFile { offset: read_offset, len: LEN };
        let read_value = reader.get(read_location).unwrap().unwrap();
        let first_expected_value: u8 = (read_offset / 3 * 2).try_into().unwrap();
        let expected_value = vec![first_expected_value, first_expected_value + 1];
        assert_eq!(read_value, expected_value);
    }

    let mut handles = Vec::new();
    for round in 0..ROUNDS {
        for _ in 0..n_readers_per_phase {
            let reader = reader.clone();
            handles.push(tokio::spawn(reader_task(reader, lock.clone(), barrier.clone())));
        }

        writer.append(&data);
        writer.flush();
        {
            *lock.write().await = round as usize;
        }
        barrier.wait().await;
        data = data.into_iter().map(|x| x + 2).collect();
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[test]
fn reader_when_writer_is_out_of_scope() {
    let dir = tempdir().unwrap();
    let offset = 0;
    let (mut writer, reader) = open_file::<NoVersionValueWrapper<Vec<u8>>>(
        get_mmap_file_test_config(),
        dir.path().to_path_buf().join("test_reader_when_writer_is_out_of_scope"),
        offset,
    )
    .unwrap();
    let data = vec![1, 2, 3];

    let location_in_file = writer.append(&data);
    let res = reader.get(location_in_file).unwrap().unwrap();
    assert_eq!(res, data);

    drop(writer);
    let res = reader.get(location_in_file).unwrap().unwrap();
    assert_eq!(res, data);

    dir.close().unwrap();
}
