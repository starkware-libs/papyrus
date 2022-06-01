use papyrus_lib::{
    gateway::run_server,
    storage::create_store_access,
    sync::{CentralSource, StateSync},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let (reader, writer) = create_store_access()?;

    // Network interface.
    let central_source = CentralSource::new()?;

    // Sync.
    let mut sync = StateSync::new(central_source, writer);
    let sync_thread = tokio::spawn(async move { sync.run().await });

    // Pass reader to storage.
    let (run_server_res, sync_thread_res) = tokio::join!(run_server(reader), sync_thread);
    run_server_res?;
    sync_thread_res??;

    Ok(())
}
