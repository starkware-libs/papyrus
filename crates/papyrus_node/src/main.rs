use clap::Parser;
use log::info;
// use papyrus_gateway::run_server;
// use papyrus_monitoring_gateway::run_server as monitoring_run_server;
use papyrus_node::config::load_config;
use papyrus_storage::open_storage;
use papyrus_sync::{CentralSource, StateSync, StateSyncError};
use tokio;
use tokio::task::JoinHandle;

#[derive(Parser)]
struct Args {
    /// If set, the node will sync and get new blocks and state diffs from its sources.
    #[clap(short, long, value_parser)]
    no_sync: bool,

    /// If set, use this path for the storage instead of the one in the config.
    #[clap(short, long, value_parser)]
    storage_path: Option<String>,

    /// Whether to sync from other peers or a central source.
    #[clap(short, long, value_parser, default_value_t = false)]
    p2p_source_mode: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    log4rs::init_file("config/log4rs.yaml", Default::default()).unwrap();
    info!("Booting up.");

    let mut config = load_config("config/config.ron")?;
    if let Some(storage_path_str) = args.storage_path {
        config.storage.db_config.path = storage_path_str;
    }

    let (storage_reader, storage_writer) = open_storage(config.storage.db_config)?;

    // P2P.
    let (mut network_client, mut network_events, network_event_loop) = papyrus_p2p::new().await?;
    let network_event_loop_handler = tokio::spawn(network_event_loop.run());

    // Responder.
    let responder = papyrus_p2p::responder::Responder::new(
        storage_reader.clone(),
        network_events,
        network_client.clone(),
    );
    let responder_handler = tokio::spawn(responder.run());

    // Network interface.
    let p2p_source = papyrus_sync::P2PSource::new(network_client);
    let central_source = CentralSource::new(config.central)?;

    // Sync.
    let mut sync_thread_central_opt: Option<JoinHandle<anyhow::Result<(), StateSyncError>>> = None;
    let mut sync_thread_p2p_opt: Option<
        JoinHandle<anyhow::Result<(), papyrus_sync::p2p::StateSyncError>>,
    > = None;
    if !args.no_sync {
        if args.p2p_source_mode {
            let mut sync_p2p = papyrus_sync::p2p::StateSync::new(
                storage_reader.clone(),
                storage_writer,
                p2p_source,
            );
            sync_thread_p2p_opt = Some(tokio::spawn(async move { sync_p2p.run().await }));
        } else {
            let mut sync =
                StateSync::new(config.sync, central_source, storage_reader.clone(), storage_writer);
            sync_thread_central_opt = Some(tokio::spawn(async move { sync.run().await }));
        }
    }

    // Pass reader to storage.
    // let (_, server_handle) = run_server(config.gateway, storage_reader.clone()).await?;
    // let (_, monitoring_server_handle) =
    // monitoring_run_server(config.monitoring_gateway, storage_reader.clone()).await?;

    if let Some(sync_thread) = sync_thread_central_opt {
        // let (_, _, sync_thread_res) =
        // tokio::join!(server_handle, monitoring_server_handle, sync_thread);
        // sync_thread_res??;
        let sync_thread_res =
            tokio::try_join!(sync_thread, network_event_loop_handler, responder_handler);
        sync_thread_res?.0?;
    } else {
        let _r = tokio::try_join!(network_event_loop_handler, responder_handler);
        // tokio::join!(server_handle, monitoring_server_handle);
    }

    if let Some(sync_thread) = sync_thread_p2p_opt {
        // let (_, _, sync_thread_res) =
        // tokio::join!(server_handle, monitoring_server_handle, sync_thread);
        // sync_thread_res??;
        let sync_thread_res = tokio::try_join!(sync_thread);
        sync_thread_res?.0?;
    }

    Ok(())
}
