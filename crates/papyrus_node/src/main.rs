use std::env::args;
use std::sync::Arc;

use papyrus_common::BlockHashAndNumber;
use papyrus_config::ConfigError;
use papyrus_monitoring_gateway::MonitoringServer;
use papyrus_node::config::NodeConfig;
use papyrus_node::version::VERSION_FULL;
use papyrus_rpc::run_server;
use papyrus_storage::{open_storage, StorageReader, StorageWriter};
use papyrus_sync::sources::base_layer::{BaseLayerSourceError, EthereumBaseLayerSource};
use papyrus_sync::sources::central::{CentralError, CentralSource};
use papyrus_sync::{StateSync, StateSyncError};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;
use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

// TODO(yair): Add to config.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

async fn run_threads(config: NodeConfig) -> anyhow::Result<()> {
    let (storage_reader, storage_writer) = open_storage(config.storage.db_config.clone())?;

    // Monitoring server.
    let monitoring_server = MonitoringServer::new(
        config.monitoring_gateway.clone(),
        config.get_config_representation()?,
        storage_reader.clone(),
        VERSION_FULL,
    )?;
    let monitoring_server_handle = monitoring_server.spawn_server().await;

    // The sync is the only writer of the syncing state.
    let shared_highest_block = Arc::new(RwLock::new(None));
    // JSON-RPC server.
    let (_, server_handle) =
        run_server(&config.rpc, shared_highest_block.clone(), storage_reader.clone(), VERSION_FULL)
            .await?;
    let server_handle_future = tokio::spawn(server_handle.stopped());

    // Sync task.
    let sync_future =
        run_sync(config, shared_highest_block, storage_reader.clone(), storage_writer);
    let sync_handle = tokio::spawn(sync_future);

    // TODO(dvir): refactor + better error handling.
    async fn flatten_with_result<T: std::convert::Into<anyhow::Error>>(
        handle: JoinHandle<Result<(), T>>,
    ) -> anyhow::Result<()> {
        match handle.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(err)) => Err(err.into()),
            Err(err) => Err(err.into()),
        }
    }

    async fn flatten_server(handle: JoinHandle<()>) -> anyhow::Result<()> {
        match handle.await {
            Ok(_) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    tokio::try_join!(
        // We use flatten in order to try_join the inner result, not the join handle.
        flatten_server(server_handle_future),
        flatten_with_result(monitoring_server_handle),
        flatten_with_result(sync_handle)
    )?;

    return Ok(());

    async fn run_sync(
        config: NodeConfig,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
    ) -> Result<(), StateSyncError> {
        let Some(sync_config) = config.sync else { return Ok(()) };
        let central_source =
            CentralSource::new(config.central, VERSION_FULL, storage_reader.clone())
                .map_err(CentralError::ClientCreation)?;
        let base_layer_source = EthereumBaseLayerSource::new(config.base_layer)
            .map_err(|e| BaseLayerSourceError::BaseLayerSourceCreationError(e.to_string()))?;
        let mut sync = StateSync::new(
            sync_config,
            shared_highest_block,
            central_source,
            base_layer_source,
            storage_reader.clone(),
            storage_writer,
        );
        sync.run().await
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
    let config = NodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }
    configure_tracing();
    info!("Booting up.");
    run_threads(config?).await
}
