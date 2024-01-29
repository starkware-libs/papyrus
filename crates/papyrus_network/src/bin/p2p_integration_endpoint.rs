use std::time::Duration;

use clap::Parser;
use libp2p::{StreamProtocol, Swarm};
use papyrus_network::bin_utils::{build_swarm, dial};
use papyrus_network::block_headers::behaviour::Behaviour;
use papyrus_network::network_manager;
use papyrus_network::streamed_data::Config;
use papyrus_storage::{open_storage, StorageConfig};

/// A dummy P2P capable node for integration with other P2P capable nodes.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address this node listens on for incoming connections.
    #[arg(short, long)]
    listen_address: String,

    /// Address this node attempts to dial to.
    #[arg(short, long)]
    dial_address: Option<String>,

    /// Amount of time (in seconds) to wait until closing an idle connection.
    #[arg(short = 't', long, default_value_t = 1)]
    idle_connection_timeout: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config = Config {
        substream_timeout: Duration::from_secs(3600),
        protocol_name: StreamProtocol::new("/core/headers-sync/1"),
    };
    let mut swarm: Swarm<Behaviour> =
        build_swarm(args.listen_address.clone(), args.idle_connection_timeout, config);
    if let Some(dial_address) = args.dial_address.as_ref() {
        dial(&mut swarm, dial_address);
    }
    let (storage_reader, _storage_writer) = open_storage(StorageConfig::default()).unwrap();
    let network_manager = network_manager::NetworkManager::new(swarm, storage_reader);
    network_manager.run().await;
}
