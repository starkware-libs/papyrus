use std::sync::{Arc, Barrier};

use tempfile::tempdir;

use super::*;

fn get_test_config() -> MmapFileConfig {
    MmapFileConfig {
        max_size: 1 << 24,       // 16MB
        growth_step: 1 << 20,    // 1MB
        max_object_size: 1 << 8, // 256B
    }
}

#[test]
fn config_validation() {
    let mut config = get_test_config();
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
    let (mut writer, reader) =
        open_file(get_test_config(), dir.path().to_path_buf().join("test_write_read")).unwrap();
    let data: Vec<u8> = vec![1, 2, 3];
    let offset = 0;

    let len = writer.insert(offset, &data);
    let res_writer = writer.get(LocationInFile { offset, len }).unwrap();
    assert_eq!(res_writer, data);

    let res: Vec<u8> = reader.get(LocationInFile { offset, len }).unwrap();
    assert_eq!(res, data);

    let reader_clone = reader;
    let res: Vec<u8> = reader_clone.get(LocationInFile { offset, len }).unwrap();
    assert_eq!(res, data);

    dir.close().unwrap();
}
// TODO: test writing and reading from different locations.
#[test]
fn concurrent_reads() {
    let dir = tempdir().unwrap();
    let (mut writer, reader) =
        open_file(get_test_config(), dir.path().to_path_buf().join("test_concurrent_reads"))
            .unwrap();
    let data: Vec<u8> = vec![1, 2, 3];
    let offset = 0;

    let len = writer.insert(offset, &data);
    let location_in_file = LocationInFile { offset, len };

    let num_threads = 50;
    let mut handles = vec![];

    for _ in 0..num_threads {
        let reader = reader.clone();
        let handle = std::thread::spawn(move || reader.get(location_in_file).unwrap());
        handles.push(handle);
    }

    for handle in handles {
        let res: Vec<u8> = handle.join().unwrap();
        assert_eq!(res, data);
    }

    dir.close().unwrap();
}

#[test]
fn concurrent_reads_single_write() {
    let dir = tempdir().unwrap();
    let (mut writer, reader) = open_file(
        get_test_config(),
        dir.path().to_path_buf().join("test_concurrent_reads_single_write"),
    )
    .unwrap();
    let first_data: Vec<u8> = vec![1, 2, 3];
    let second_data: Vec<u8> = vec![3, 2, 1];
    let offset = 0;
    let len = writer.insert(offset, &first_data);
    writer.flush();
    let first_location = LocationInFile { offset, len };
    let second_location = LocationInFile { offset: offset + len, len };

    let n = 10;
    let barrier = Arc::new(Barrier::new(n + 1));
    let mut handles = Vec::with_capacity(n);

    for _ in 0..n {
        let reader = reader.clone();
        let reader_barrier = barrier.clone();
        let first_data = first_data.clone();
        handles.push(std::thread::spawn(move || {
            assert_eq!(
                <FileReader as Reader<Vec<u8>>>::get(&reader, first_location).unwrap(),
                first_data
            );
            reader_barrier.wait();
            // readers wait for the writer to write the value.
            reader_barrier.wait();
            reader.get(second_location).unwrap()
        }));
    }
    // Writer waits for all readers to read the first value.
    barrier.wait();
    writer.insert(offset + len, &second_data);
    writer.flush();
    // Allow readers to proceed reading the second value.
    barrier.wait();

    for handle in handles {
        let res: Vec<u8> = handle.join().unwrap();
        assert_eq!(res, second_data);
    }
}

#[test]
fn grow_file() {
    let data: Vec<u8> = vec![1, 2];
    let serialization_size = StorageSerdeEx::serialize(&data).unwrap().len();
    let dir = tempdir().unwrap();
    let config = MmapFileConfig {
        max_size: 10 * serialization_size,
        max_object_size: serialization_size, // 3 (len + data)
        growth_step: serialization_size + 1, // 4
    };

    let file_path = dir.path().to_path_buf().join("test_grow_file");
    {
        let file =
            OpenOptions::new().read(true).write(true).create(true).open(file_path.clone()).unwrap();
        // file_size = 0, offset = 0
        assert_eq!(file.metadata().unwrap().len(), 0);

        let (mut writer, _) = open_file(config.clone(), file_path.clone()).unwrap();
        // file_size = 4 (growth_step), offset = 0
        let mut file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, config.growth_step as u64);

        let mut offset = 0;
        offset += writer.insert(offset, &data);
        // file_size = 8 (2 * growth_step), offset = 3 (serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 2 * config.growth_step as u64);

        offset += writer.insert(offset, &data);
        // file_size = 12 (3 * growth_step), offset = 6 (2 * serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 3 * config.growth_step as u64);

        offset += writer.insert(offset, &data);
        // file_size = 12 (3 * growth_step), offset = 9 (3 * serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 3 * config.growth_step as u64);

        writer.insert(offset, &data);
        // file_size = 16 (4 * growth_step), offset = 12 (4 * serialization_size)
        file_size = file.metadata().unwrap().len();
        assert_eq!(file_size, 4 * config.growth_step as u64);
    }

    let file =
        OpenOptions::new().read(true).write(true).create(true).open(file_path.clone()).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 4 * config.growth_step as u64);
    let _ = open_file::<Vec<u8>>(config.clone(), file_path).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 4 * config.growth_step as u64);

    dir.close().unwrap();
}
