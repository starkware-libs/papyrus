use tempfile::tempdir;

use super::*;

#[test]
fn raw_insert() {
    let dir = tempdir().unwrap();
    let mut file: LargeFile<Vec<u8>> =
        open_mmaped_file(dir.path().to_path_buf().join("test_file_raw_insert"));
    let data = vec![1, 2, 3];
    let offset = 0;
    let location = LocationInFile { offset, len: data.len() };

    file.insert_raw(offset, &data);
    let res = file.get_raw(location);
    assert_eq!(res, data.as_slice());

    dir.close().unwrap();
}

#[test]
fn objects() {
    let dir = tempdir().unwrap();
    let mut file: LargeFile<Vec<u8>> =
        open_mmaped_file(dir.path().to_path_buf().join("test_file_object_insert"));
    let data = vec![1, 2, 3];
    let offset = 0;

    let serialization_len = file.insert(offset, &data);
    let res = file.get(LocationInFile { offset, len: serialization_len });
    assert_eq!(res, data);

    let res = file.get_raw(LocationInFile { offset: offset + 1, len: data.len() });
    assert_eq!(res, data.as_slice());

    dir.close().unwrap();
}
