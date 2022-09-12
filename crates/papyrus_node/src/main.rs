use clap::{ArgAction, Parser};
use log::info;
use papyrus_monitoring_gateway::run_server as monitoring_run_server;
use papyrus_node::config::load_config;
use papyrus_storage::open_storage;
use papyrus_sync::{CentralSource, StateSync, StateSyncError};
use tokio::task::JoinHandle;

#[derive(Parser)]
struct Args {
    /// If set, the node will sync and get new blocks and state diffs from the source.
    #[clap(short, long, value_parser, action = ArgAction::SetTrue)]
    no_sync: bool,

    /// If set, use this path for the storage instead of the one in the config.
    #[clap(short, long, value_parser)]
    storage_path: Option<String>,
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

    // Network interface.
    let central_source = CentralSource::new(config.central)?;

    // Sync.
    let mut sync_thread_opt: Option<JoinHandle<anyhow::Result<(), StateSyncError>>> = None;
    if !args.no_sync {
        let mut sync =
            StateSync::new(config.sync, central_source, storage_reader.clone(), storage_writer);
        sync_thread_opt = Some(tokio::spawn(async move { sync.run().await }));
    }

    // Pass reader to storage.
    let (_, monitoring_server_handle) =
        monitoring_run_server(config.monitoring_gateway, storage_reader.clone()).await?;
    if let Some(sync_thread) = sync_thread_opt {
        let (_, sync_thread_res) = tokio::join!(monitoring_server_handle, sync_thread);
        sync_thread_res??;
    } else {
        tokio::join!(monitoring_server_handle);
    }

    Ok(())
}
