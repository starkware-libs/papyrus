// Run the following two commands in different terminals:
// 1. LISTENER_ADDRESS=/ip4/127.0.0.1/tcp/8080 cargo run -p papyrus_network --bin node
// 2. OTHER_ADDRESS=/ip4/127.0.0.1/tcp/8080 LISTENER_ADDRESS=/ip4/127.0.0.1/tcp/8082 cargo run -p
//    papyrus_network --bin node
use std::env::var;
use std::str::FromStr;

use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{noise, yamux, Multiaddr, SwarmBuilder};

#[tokio::main]
async fn main() {
    let listener_address = var("LISTENER_ADDRESS")
        .expect("Set the address of this node with the env var LISTENER_ADDRESS");
    let listener_address =
        Multiaddr::from_str(&listener_address).expect("Address parsing error in LISTENER_ADDRESS");

    let key_pair = Keypair::generate_ed25519();
    // let public_key = key_pair.public();
    // let transport = tcp::tokio::Transport::default()
    //     .upgrade(upgrade::Version::V1)
    //     .authenticate(noise::Config::new(&key_pair).unwrap())
    //     .multiplex(yamux::Config::default())
    //     .boxed();

    // let peer_id = public_key.to_peer_id();
    let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
        .with_tokio()
        .with_tcp(Default::default(), noise::Config::new, yamux::Config::default)
        .unwrap()
        .with_behaviour(|keypair| {
            libp2p::identify::Behaviour::new(libp2p::identify::Config::new(
                "1".to_owned(),
                keypair.public(),
            ))
        })
        .unwrap()
        .build();
    swarm.listen_on(listener_address).unwrap();

    let other_address_opt = var("OTHER_ADDRESS").ok().map(|address| {
        Multiaddr::from_str(&address).expect("Address parsing error in LISTENER_ADDRESS")
    });

    if let Some(other_address) = other_address_opt {
        swarm.dial(DialOpts::unknown_peer_id().address(other_address).build()).unwrap();
    }

    while let Some(event) = swarm.next().await {
        println!("Event: {:?}", event);
    }
}
