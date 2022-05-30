mod info;

pub use self::info::{info_prepare, InfoReader, InfoWriter};

pub struct StorageComponents {
    pub info_reader: InfoReader,
    pub info_writer: InfoWriter,
}
