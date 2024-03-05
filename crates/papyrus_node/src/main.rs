#[cfg(test)]
mod main_test;

use std::env::args;
use std::future::{self, pending};
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc::Sender;
use futures::future::BoxFuture;
use futures::FutureExt;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_common::BlockHashAndNumber;
use papyrus_config::presentation::get_config_presentation;
use papyrus_config::validators::config_validate;
use papyrus_config::ConfigError;
use papyrus_monitoring_gateway::MonitoringServer;
use papyrus_network::network_manager::NetworkError;
use papyrus_network::{network_manager, NetworkConfig, Protocol, Query, ResponseReceivers};
use papyrus_node::config::NodeConfig;
use papyrus_node::version::VERSION_FULL;
use papyrus_p2p_sync::{P2PSync, P2PSyncConfig, P2PSyncError};
use papyrus_rpc::run_server;
use papyrus_storage::{open_storage, update_storage_metrics, StorageReader, StorageWriter};
use papyrus_sync::sources::base_layer::{BaseLayerSourceError, EthereumBaseLayerSource};
use papyrus_sync::sources::central::{CentralError, CentralSource, CentralSourceConfig};
use papyrus_sync::sources::pending::PendingSource;
use papyrus_sync::{StateSync, StateSyncError, SyncConfig};
use starknet_api::block::BlockHash;
use starknet_api::hash::{StarkFelt, GENESIS_HASH};
use starknet_api::stark_felt;
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingBlockOrDeprecated};
use starknet_client::reader::PendingData;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::metadata::LevelFilter;
use tracing::{debug_span, error, info, warn, Instrument};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

// TODO(yair): Add to config.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

// TODO(dvir): add this to config.
// Duration between updates to the storage metrics (those in the collect_storage_metrics function).
const STORAGE_METRICS_UPDATE_INTERVAL: Duration = Duration::from_secs(10);

async fn run_threads(config: NodeConfig) -> anyhow::Result<()> {
    let (storage_reader, storage_writer) = open_storage(config.storage.clone())?;

    let storage_metrics_handle = if config.monitoring_gateway.collect_metrics {
        spawn_storage_metrics_collector(storage_reader.clone(), STORAGE_METRICS_UPDATE_INTERVAL)
    } else {
        tokio::spawn(future::pending())
    };

    // Monitoring server.
    let monitoring_server = MonitoringServer::new(
        config.monitoring_gateway.clone(),
        get_config_presentation(&config, true)?,
        get_config_presentation(&config, false)?,
        storage_reader.clone(),
        VERSION_FULL,
    )?;
    let monitoring_server_handle = monitoring_server.spawn_server().await;

    // The sync is the only writer of the syncing state.
    let shared_highest_block = Arc::new(RwLock::new(None));
    let pending_data = Arc::new(RwLock::new(PendingData {
        // The pending data might change later to DeprecatedPendingBlock, depending on the response
        // from the feeder gateway.
        block: PendingBlockOrDeprecated::Current(PendingBlock {
            parent_block_hash: BlockHash(stark_felt!(GENESIS_HASH)),
            ..Default::default()
        }),
        ..Default::default()
    }));
    let pending_classes = Arc::new(RwLock::new(PendingClasses::default()));

    // JSON-RPC server.
    let (_, server_handle) = run_server(
        &config.rpc,
        shared_highest_block.clone(),
        pending_data.clone(),
        pending_classes.clone(),
        storage_reader.clone(),
        VERSION_FULL,
    )
    .await?;
    let server_handle_future = tokio::spawn(server_handle.stopped());

    // P2P network.
    let (network_future, maybe_query_sender_and_response_receivers) =
        run_network(config.network.clone(), storage_reader.clone());
    let network_handle = tokio::spawn(network_future);

    // Sync task.
    let (sync_future, p2p_sync_future) = match (config.sync, config.p2p_sync) {
        (Some(_), Some(_)) => {
            panic!("One of --sync.#is_none or --p2p_sync.#is_none must be turned on");
        }
        (Some(sync_config), None) => {
            let configs = (sync_config, config.central, config.base_layer);
            let storage = (storage_reader.clone(), storage_writer);
            let sync_fut =
                run_sync(configs, shared_highest_block, pending_data, pending_classes, storage);
            (sync_fut.boxed(), pending().boxed())
        }
        (None, Some(p2p_sync_config)) => {
            let (query_sender, response_receivers) = maybe_query_sender_and_response_receivers
                .expect("If p2p sync is enabled, network needs to be enabled too");
            (
                pending().boxed(),
                run_p2p_sync(
                    p2p_sync_config,
                    storage_reader.clone(),
                    storage_writer,
                    query_sender,
                    response_receivers,
                )
                .boxed(),
            )
        }
        (None, None) => (pending().boxed(), pending().boxed()),
    };
    let sync_handle = tokio::spawn(sync_future);
    let p2p_sync_handle = tokio::spawn(p2p_sync_future);

    tokio::select! {
        res = storage_metrics_handle => {
            error!("collecting storage metrics stopped.");
            res?
        }
        res = server_handle_future => {
            error!("RPC server stopped.");
            res?
        }
        res = monitoring_server_handle => {
            error!("Monitoring server stopped.");
            res??
        }
        res = sync_handle => {
            error!("Sync stopped.");
            res??
        }
        res = p2p_sync_handle => {
            error!("P2P Sync stopped.");
            res??
        }
        res = network_handle => {
            error!("Network stopped.");
            res??
        }
    };
    error!("Task ended with unexpected Ok.");
    return Ok(());

    async fn run_sync(
        configs: (SyncConfig, CentralSourceConfig, EthereumBaseLayerConfig),
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        pending_data: Arc<RwLock<PendingData>>,
        pending_classes: Arc<RwLock<PendingClasses>>,
        storage: (StorageReader, StorageWriter),
    ) -> Result<(), StateSyncError> {
        let (sync_config, central_config, base_layer_config) = configs;
        let (storage_reader, storage_writer) = storage;
        let central_source =
            CentralSource::new(central_config.clone(), VERSION_FULL, storage_reader.clone())
                .map_err(CentralError::ClientCreation)?;
        let pending_source = PendingSource::new(central_config, VERSION_FULL)
            .map_err(CentralError::ClientCreation)?;
        let base_layer_source = EthereumBaseLayerSource::new(base_layer_config)
            .map_err(|e| BaseLayerSourceError::BaseLayerSourceCreationError(e.to_string()))?;
        let mut sync = StateSync::new(
            sync_config,
            shared_highest_block,
            pending_data,
            pending_classes,
            central_source,
            pending_source,
            base_layer_source,
            storage_reader.clone(),
            storage_writer,
        );
        sync.run().await
    }

    async fn run_p2p_sync(
        p2p_sync_config: P2PSyncConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        query_sender: Sender<Query>,
        response_receivers: ResponseReceivers,
    ) -> Result<(), P2PSyncError> {
        let sync = P2PSync::new(
            p2p_sync_config,
            storage_reader,
            storage_writer,
            query_sender,
            response_receivers,
        );
        sync.run().await
    }
}

type NetworkRunReturn =
    (BoxFuture<'static, Result<(), NetworkError>>, Option<(Sender<Query>, ResponseReceivers)>);

fn run_network(config: Option<NetworkConfig>, storage_reader: StorageReader) -> NetworkRunReturn {
    let Some(network_config) = config else { return (pending().boxed(), None) };
    let mut network_manager =
        network_manager::NetworkManager::new(network_config.clone(), storage_reader.clone());
    let (query_sender, response_receivers) =
        network_manager.register_subscriber(vec![Protocol::SignedBlockHeader]);
    (network_manager.run().boxed(), Some((query_sender, response_receivers)))
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

fn spawn_storage_metrics_collector(
    storage_reader: StorageReader,
    update_interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(
        async move {
            loop {
                if let Err(error) = update_storage_metrics(&storage_reader) {
                    warn!("Failed to update storage metrics: {error}");
                }
                tokio::time::sleep(update_interval).await;
            }
        }
        .instrument(debug_span!("collect_storage_metrics")),
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = NodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }

    configure_tracing();

    let config = config?;
    if let Err(errors) = config_validate(&config) {
        error!("{}", errors);
        exit(1);
    }

    info!("Booting up.");
    run_threads(config).await
}
