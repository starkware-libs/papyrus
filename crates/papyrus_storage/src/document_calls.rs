//! This module is used to log calls to the storage system. This is useful for benchmarking
//! and used in the storage_benchmark tool.

use std::fs::File;
use std::io::Write;
use std::sync::Mutex;

use lazy_static::lazy_static;

pub use crate::StorageQuery;

// TODO(dvir): consider enabling the user to choose the file path using an environment variable.
const QUERY_FILE_PATH: &str = "./document_calls.txt";

lazy_static! {
    pub(crate) static ref QUERY_FILE: Mutex<File> = Mutex::new(
        File::create(QUERY_FILE_PATH).expect("Failed to create document_calls.txt file")
    );
}

// Adds a query to the document_calls file.
pub(crate) fn add_query(query: StorageQuery) {
    let query_string = serde_json::to_string(&query).expect("Failed to serialize query");
    let mut file = QUERY_FILE.lock().expect("Failed to lock file");
    file.write_all(query_string.as_bytes()).expect("Failed to write query to file");
    file.write_all(b"\n").expect("Failed to write a new line to file");
    file.flush().expect("Failed to flush file");
}
