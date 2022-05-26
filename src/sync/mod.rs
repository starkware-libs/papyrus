mod sources;

use log::info;

// use crate::storage::DataStoreHandle;

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct StateSync {
    // #[allow(dead_code)]
// storage: DataStoreHandle,
}

impl StateSync {
    pub fn new() -> StateSync {
        StateSync {}
    }
    pub fn run(&mut self) {
        info!("State sync started.");
        todo!("Not implemented yet.");
    }
}
