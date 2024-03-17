use std::str::FromStr;
use std::time::Duration;

use clap::Parser;
use libp2p::Multiaddr;
use papyrus_network::{network_manager, NetworkConfig};
use papyrus_storage::{open_storage, StorageConfig};

/// A dummy P2P capable node for integration with other P2P capable nodes.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port this node listens on for incoming tcp connections.
    #[arg(short, long)]
    tcp_port: u16,

    /// Port this node listens on for incoming quic connections.
    #[arg(short, long)]
    quic_port: u16,

    /// Multiaddress of the peer node.
    #[arg(short, long)]
    peer_multiaddr: String,

    /// Amount of time (in seconds) to wait until closing an idle connection.
    #[arg(short = 't', long, default_value_t = 1)]
    idle_connection_timeout: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let (storage_reader, _storage_writer) =
        open_storage(StorageConfig::default()).expect("failed to open storage");
    let network_manager = network_manager::NetworkManager::new(
        NetworkConfig {
            tcp_port: args.tcp_port,
            quic_port: args.quic_port,
            session_timeout: Duration::from_secs(10),
            idle_connection_timeout: Duration::from_secs(args.idle_connection_timeout),
            header_buffer_size: 100000,
            peer_multiaddr: Some(
                Multiaddr::from_str(&args.peer_multiaddr).expect("failed to parse peer multiaddr"),
            ),
        },
        storage_reader,
    );
    network_manager.run().await.expect("Network manager failed");
}
