mod sources;

use log::info;

use crate::storage::StorageHandle;

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct StateSync {
    #[allow(dead_code)]
    storage: StorageHandle,
}

impl StateSync {
    pub fn new(storage: StorageHandle) -> StateSync {
        StateSync { storage }
    }
    pub fn run(&mut self) {
        info!("State sync started.");
        todo!("Not implemented yet.");
    }
}
