use papyrus_lib::{
    gateway::run_server,
    storage::components::StorageComponents,
    sync::{CentralSource, StateSync},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut path = std::env::current_exe()?;
    path.pop();
    path.push("data");
    let storage_components = StorageComponents::new(path.as_path())?;

    // Network interface.
    let central_source = CentralSource::new()?;

    // Sync.
    let mut sync = StateSync::new(central_source, storage_components.block_storage_writer);
    let sync_thread = tokio::spawn(async move { sync.run().await });

    // Pass reader to storage.
    let (run_server_res, sync_thread_res) = tokio::join!(
        run_server(storage_components.block_storage_reader),
        sync_thread
    );
    run_server_res?;
    sync_thread_res??;

    Ok(())
}
