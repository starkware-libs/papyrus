use log::info;
use papyrus_gateway::run_server;
use papyrus_monitoring_gateway::run_server as monitoring_run_server;
use papyrus_node::config::Config;
use papyrus_storage::{open_storage, StorageReader, StorageWriter};
use papyrus_sync::{CentralSource, StateSync, StateSyncError};

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
        match config.sync {
            None => Ok(()),
            Some(sync_config) => match CentralSource::new(config.central.clone()) {
                Ok(central_source) => {
                    let mut sync = StateSync::new(
                        sync_config,
                        central_source,
                        storage_reader.clone(),
                        storage_writer,
                    );
                    sync.run().await
                }
                Err(err) => Err(StateSyncError::ClientCreation(err)),
            },
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    log4rs::init_file("config/log4rs.yaml", Default::default())?;
    info!("Booting up.");
    let config = Config::load()?;
    run_threads(config).await
}
