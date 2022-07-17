use log::info;
use starknet_node::config::load_config;
use starknet_node::gateway::run_server;
use starknet_node::storage::StorageComponents;
use starknet_node::sync::{CentralSource, StateSync};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    log4rs::init_file("config/log4rs.yaml", Default::default()).unwrap();
    info!("Booting up.");

    let config = load_config("config/config.ron")?;

    let storage_components = StorageComponents::new(config.storage)?;

    // Network interface.
    let central_source = CentralSource::new(config.central)?;

    // Sync.
    let mut sync = StateSync::new(
        config.sync,
        central_source,
        storage_components.block_storage_reader.clone(),
        storage_components.block_storage_writer,
    );
    let sync_thread = tokio::spawn(async move { sync.run().await });

    // Pass reader to storage.
    let (run_server_res, sync_thread_res) = tokio::join!(
        run_server(config.gateway, storage_components.block_storage_reader.clone(),),
        sync_thread
    );
    run_server_res?;
    sync_thread_res??;

    Ok(())
}
