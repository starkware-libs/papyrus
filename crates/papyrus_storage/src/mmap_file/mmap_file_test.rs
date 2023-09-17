use tempfile::tempdir;

use super::*;

#[test]
fn write_read() {
    let dir = tempdir().unwrap();
    let (mut writer, reader) = open_file(dir.path().to_path_buf().join("test_file_write_read"));
    let data: Vec<u8> = vec![1, 2, 3];
    let offset = 0;

    let len = writer.insert(offset, &data);
    let res_writer = writer.get(LocationInFile { offset, len });
    assert_eq!(res_writer, data);

    let res: Vec<u8> = reader.get(LocationInFile { offset, len });
    assert_eq!(res, data);

    let reader_clone = reader;
    let res: Vec<u8> = reader_clone.get(LocationInFile { offset, len });
    assert_eq!(res, data);

    dir.close().unwrap();
}
// TODO: test writing and reading from different locations.
#[test]
fn concurrent_reads() {
    let dir = tempdir().unwrap();
    let (mut writer, reader) =
        open_file(dir.path().to_path_buf().join("test_file_concurrent_reads"));
    let data: Vec<u8> = vec![1, 2, 3];
    let offset = 0;

    let len = writer.insert(offset, &data);
    let location_in_file = LocationInFile { offset, len };

    let num_threads = 50;
    let mut handles = vec![];

    for _ in 0..num_threads {
        let reader = reader.clone();
        let handle = std::thread::spawn(move || reader.get(location_in_file));
        handles.push(handle);
    }

    for handle in handles {
        let res: Vec<u8> = handle.join().unwrap();
        assert_eq!(res, data);
    }

    dir.close().unwrap();
}
