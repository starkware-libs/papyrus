use std::str::FromStr;
use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{noise, yamux, Multiaddr, Swarm, SwarmBuilder};
use tracing::debug;

pub fn build_swarm<Behaviour: NetworkBehaviour>(
    listen_addresses: Vec<String>,
    idle_connection_timeout: Duration,
    behaviour: Behaviour,
) -> Swarm<Behaviour>
where
{
    let listen_addresses = listen_addresses.iter().map(|listen_address| {
        Multiaddr::from_str(listen_address)
            .unwrap_or_else(|_| panic!("Unable to parse address {}", listen_address))
    });
    debug!("Creating swarm with listen addresses: {:?}", listen_addresses);

    let key_pair = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
        .with_tokio()
        .with_tcp(Default::default(), noise::Config::new, yamux::Config::default)
        .expect("Error building TCP transport")
        // TODO: quic transpot does not work (failure appears in the command line when running in debug mode)
        // .with_quic()
        .with_behaviour(|_| behaviour)
        .expect("Error while building the swarm")
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(idle_connection_timeout))
        .build();
    for listen_address in listen_addresses {
        swarm
            .listen_on(listen_address.clone())
            .unwrap_or_else(|_| panic!("Error while binding to {}", listen_address));
    }
    swarm
}

pub fn dial<Behaviour>(swarm: &mut Swarm<Behaviour>, dial_address_str: &str)
where
    Behaviour: NetworkBehaviour,
{
    let dial_address = Multiaddr::from_str(dial_address_str)
        .unwrap_or_else(|_| panic!("Unable to parse address {}", dial_address_str));
    swarm
        .dial(DialOpts::unknown_peer_id().address(dial_address).build())
        .unwrap_or_else(|_| panic!("Error while dialing {}", dial_address_str));
}
