use std::env::args;

use papyrus_gateway::run_server;
use papyrus_monitoring_gateway::run_server as monitoring_run_server;
use papyrus_node::config::Config;
use papyrus_storage::{open_storage, StorageReader, StorageWriter};
use papyrus_sync::{CentralError, CentralSource, StateSync, StateSyncError};
use tracing::info;
use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

// TODO(yair): Add to config.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

async fn run_threads(config: Config) -> anyhow::Result<()> {
    let (storage_reader, storage_writer) = open_storage(config.storage.db_config.clone())?;

    let (_, server_future) = run_server(&config.gateway, storage_reader.clone()).await?;
    let (_, monitoring_server_future) = monitoring_run_server(
        config.get_config_representation()?,
        config.monitoring_gateway.clone(),
        storage_reader.clone(),
    )
    .await?;
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

// TODO(yair): add dynamic level filtering.
// TODO(dan): filter out logs from dependencies (happens when RUST_LOG=DEBUG)
// TODO(yair): define and implement configurable filtering.
fn configure_tracing() {
    let fmt_layer = fmt::layer().compact().with_target(false);
    let level_filter_layer =
        EnvFilter::builder().with_default_directive(DEFAULT_LEVEL.into()).from_env_lossy();

    // This sets a single subscriber to all of the threads. We may want to implement different
    // subscriber for some threads and use set_global_default instead of init.
    tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load(args().collect())?;
    configure_tracing();
    info!("Booting up.");
    run_threads(config).await
}
