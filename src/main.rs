use std::fs;

use papyrus_lib::gateway::run_server;
use papyrus_lib::storage::components::{StorageComponents, StorageConfig};
use papyrus_lib::sync::{CentralSource, CentralSourceConfig, StateSync};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Config {
    storage: StorageConfig,
    central: CentralSourceConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config_path = "config.ron";
    let config_contents =
        fs::read_to_string(config_path).expect("Something went wrong reading the file");
    let config: Config = ron::from_str(&config_contents)?;

    let storage_components = StorageComponents::new(config.storage)?;

    // Network interface.
    let central_source = CentralSource::new(config.central)?;

    // Sync.
    let mut sync = StateSync::new(
        central_source,
        storage_components.block_storage_reader.clone(),
        storage_components.block_storage_writer,
    );
    let sync_thread = tokio::spawn(async move { sync.run().await });

    // Pass reader to storage.
    let (run_server_res, sync_thread_res) = tokio::join!(
        run_server(storage_components.block_storage_reader.clone()),
        sync_thread
    );
    run_server_res?;
    sync_thread_res??;

    Ok(())
}
