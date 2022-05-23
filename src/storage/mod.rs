mod api;
mod db_storage;
mod dummy;

pub use api::StorageHandle;
pub use db_storage::{create_storage, Storage, StorageHandleImpl};
