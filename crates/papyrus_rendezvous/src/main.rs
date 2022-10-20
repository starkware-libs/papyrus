use std::time::Duration;

use libp2p::core::{identity, PeerId};
use libp2p::futures::StreamExt;
use libp2p::identify::{Identify, IdentifyConfig, IdentifyEvent};
use libp2p::ping::{Ping, PingEvent};
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{development_transport, ping, rendezvous, NetworkBehaviour};
use log::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    log4rs::init_file("crates/papyrus_rendezvous/src/log4rs.yaml", Default::default())
        .expect("Load log config file failed");
    info!("Booting up.");

    let bytes = [0u8; 32];
    let key = identity::ed25519::SecretKey::from_bytes(bytes).unwrap();
    let identity = identity::Keypair::Ed25519(key.into());

    let mut swarm = Swarm::new(
        development_transport(identity.clone()).await.unwrap(),
        MyBehaviour {
            identify: Identify::new(IdentifyConfig::new(
                "rendezvous-example/1.0.0".to_string(),
                identity.public(),
            )),
            rendezvous: rendezvous::server::Behaviour::new(rendezvous::server::Config::default()),
            ping: Ping::new(
                ping::Config::new().with_interval(Duration::from_secs(60)).with_keep_alive(true),
            ),
        },
        PeerId::from(identity.public()),
    );

    log::info!("Local peer id: {}", swarm.local_peer_id());

    swarm.listen_on("/ip4/0.0.0.0/tcp/62649".parse().unwrap()).unwrap();

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::NewListenAddr { listener_id, address } => {
                log::info!("New listener {:?} with address {}", listener_id, address);
            }
            SwarmEvent::IncomingConnection { local_addr, send_back_addr } => {
                log::info!(
                    "Incoming connection: local_addr - {}, send_back_addr {}",
                    local_addr,
                    send_back_addr
                );
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                log::info!("Connected to {}", peer_id);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                log::info!("Disconnected from {}", peer_id);
            }
            SwarmEvent::Behaviour(MyEvent::Ping(PingEvent { peer, result })) => {
                log::debug!("Ping event: peer - {}, result - {:#?}", peer, result);
            }
            SwarmEvent::Behaviour(MyEvent::Identify(IdentifyEvent::Received { peer_id, info })) => {
                log::info!("Identify Received event: peer_id - {}, info - {:?}", peer_id, info);
            }
            SwarmEvent::Behaviour(MyEvent::Identify(IdentifyEvent::Sent { peer_id })) => {
                log::info!("Identify Sent event: peer_id - {}", peer_id);
            }
            SwarmEvent::Behaviour(MyEvent::Rendezvous(
                rendezvous::server::Event::PeerRegistered { peer, registration },
            )) => {
                log::info!("Peer {} registered for namespace '{}'", peer, registration.namespace);
            }
            SwarmEvent::Behaviour(MyEvent::Rendezvous(
                rendezvous::server::Event::DiscoverServed { enquirer, registrations },
            )) => {
                log::info!("Served peer {} with {} registrations", enquirer, registrations.len());
            }
            other => {
                log::error!("Unhandled {:?}", other);
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
enum MyEvent {
    Rendezvous(rendezvous::server::Event),
    Ping(PingEvent),
    Identify(IdentifyEvent),
}

impl From<rendezvous::server::Event> for MyEvent {
    fn from(event: rendezvous::server::Event) -> Self {
        MyEvent::Rendezvous(event)
    }
}

impl From<PingEvent> for MyEvent {
    fn from(event: PingEvent) -> Self {
        MyEvent::Ping(event)
    }
}

impl From<IdentifyEvent> for MyEvent {
    fn from(event: IdentifyEvent) -> Self {
        MyEvent::Identify(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "MyEvent")]
struct MyBehaviour {
    identify: Identify,
    rendezvous: rendezvous::server::Behaviour,
    ping: Ping,
}
