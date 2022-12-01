use log::info;
use papyrus_gateway::run_server;
use papyrus_monitoring_gateway::run_server as monitoring_run_server;
use papyrus_node::config::Config;
use papyrus_storage::open_storage;
use papyrus_sync::{CentralSource, StateSync, StateSyncError};
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    info!("Booting up.");
    log4rs::init_file("config/log4rs.yaml", Default::default())?;
    let config = Config::load()?;

    let (storage_reader, storage_writer) = open_storage(config.storage.db_config)?;

    // Network interface.
    let central_source = CentralSource::new(config.central)?;

    // Sync.
    let mut sync_thread_opt: Option<JoinHandle<anyhow::Result<(), StateSyncError>>> = None;
    if let Some(sync_config) = config.sync {
        let mut sync =
            StateSync::new(sync_config, central_source, storage_reader.clone(), storage_writer);
        sync_thread_opt = Some(tokio::spawn(async move { sync.run().await }));
    }

    // Pass a storage reader to the gateways.
    let (_, server_handle) = run_server(&config.gateway, storage_reader.clone()).await?;
    let (_, monitoring_server_handle) =
        monitoring_run_server(config.monitoring_gateway, storage_reader.clone()).await?;
    if let Some(sync_thread) = sync_thread_opt {
        let (_, _, sync_thread_res) =
            tokio::join!(server_handle, monitoring_server_handle, sync_thread);
        sync_thread_res??;
    } else {
        tokio::join!(server_handle, monitoring_server_handle);
    }

    Ok(())
}
