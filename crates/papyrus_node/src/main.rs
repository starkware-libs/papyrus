use std::env::args;

use papyrus_gateway::run_server;
use papyrus_monitoring_gateway::run_server as monitoring_run_server;
use papyrus_node::config::Config;
use papyrus_storage::{open_storage, StorageReader, StorageWriter};
use papyrus_sync::{CentralError, CentralSource, StateSync, StateSyncError};
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

// TODO: Add to config.
const DEFAULT_LEVEL: &str = "info";

async fn run_threads(config: Config) -> anyhow::Result<()> {
    let (storage_reader, storage_writer) = open_storage(config.storage.db_config.clone())?;

    let (_, server_future) = run_server(&config.gateway, storage_reader.clone()).await?;
    let (_, monitoring_server_future) =
        monitoring_run_server(config.monitoring_gateway.clone(), storage_reader.clone()).await?;
    let sync_future = run_sync(config, storage_reader.clone(), storage_writer);

    let server_handle = tokio::spawn(server_future);
    let monitoring_server_handle = tokio::spawn(monitoring_server_future);
    let sync_handle = tokio::spawn(sync_future);
    let (_, _, sync_result) =
        tokio::try_join!(server_handle, monitoring_server_handle, sync_handle)?;
    sync_result?;
    return Ok(());

    async fn run_sync(
        config: Config,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
    ) -> Result<(), StateSyncError> {
        if let Some(sync_config) = config.sync {
            let central_source =
                CentralSource::new(config.central.clone()).map_err(CentralError::ClientCreation)?;
            let mut sync =
                StateSync::new(sync_config, central_source, storage_reader.clone(), storage_writer);
            return sync.run().await;
        }

        Ok(())
    }
}

// TODO: add dynamic level filtering.
// TODO: filter out logs from dependencies (happens when RUST_LOG=DEBUG)
// TODO: define and implement configurable filtering.
fn configure_tracing() {
    let fmt_layer = fmt::layer().compact().with_target(false);
    let level_filter_layer =
        EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(DEFAULT_LEVEL)).unwrap();

    let subscriber = tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load(args().collect())?;
    configure_tracing();
    info!("Booting up.");
    run_threads(config).await
}
