mod sources;

use log::info;

use crate::storage::StorageHandle;

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct StateSync<S: StorageHandle> {
    #[allow(dead_code)]
    storage: S,
}

impl<S: StorageHandle> StateSync<S> {
    pub fn new(storage: S) -> Self {
        StateSync { storage }
    }
    pub async fn run(&mut self) {
        info!("State sync started.");
        todo!("Not implemented yet.");
    }
}
