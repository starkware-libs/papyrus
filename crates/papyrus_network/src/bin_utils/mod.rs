use std::str::FromStr;
use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::{Multiaddr, Swarm, SwarmBuilder};

use crate::streamed_data::Config;
use crate::PapyrusBehaviour;

pub fn build_swarm<Behaviour>(
    listen_address: String,
    idle_connection_timeout: u64,
    config: Config,
) -> Swarm<Behaviour>
where
    Behaviour: PapyrusBehaviour,
{
    let listen_address = Multiaddr::from_str(&listen_address)
        .unwrap_or_else(|_| panic!("Unable to parse address {}", listen_address));

    let key_pair = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|_| Behaviour::new(config))
        .expect("Error while building the swarm")
        .with_swarm_config(|cfg| {
            cfg.with_idle_connection_timeout(Duration::from_secs(idle_connection_timeout))
        })
        .build();
    swarm
        .listen_on(listen_address.clone())
        .unwrap_or_else(|_| panic!("Error while binding to {}", listen_address));
    swarm
}
