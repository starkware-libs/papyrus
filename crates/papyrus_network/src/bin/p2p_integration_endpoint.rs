use std::str::FromStr;
use std::time::Duration;

use clap::Parser;
use libp2p::PeerId;
use papyrus_network::{network_manager, NetworkConfig, PeerAddressConfig};
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

    /// Peer id of this node.
    #[arg(short, long)]
    peer_id: String,

    /// IP address this node listens on for incoming connections.
    #[arg(short, long)]
    ip: String,

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
            peer: Some(PeerAddressConfig {
                peer_id: PeerId::from_str(&args.peer_id).expect("Invalid peer ID"),
                tcp_port: args.tcp_port,
                ip: args.ip,
            }),
        },
        storage_reader,
    );
    network_manager.run().await.expect("Network manager failed");
}
