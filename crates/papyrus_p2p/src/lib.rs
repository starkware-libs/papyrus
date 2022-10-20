pub mod client;
mod codec;
mod event_loop;
mod protocol;
pub mod responder;
mod sync;
use std::iter;
use std::time::Duration;

use client::Client;
use event_loop::EventLoop;
use futures::channel::mpsc;
use futures_channel::mpsc::Receiver;
use libp2p::core::identity;
pub use libp2p::core::PeerId;
use libp2p::identify::{Identify, IdentifyConfig, IdentifyEvent};
use libp2p::multiaddr::Protocol;
use libp2p::ping::{Ping, PingConfig, PingEvent};
use libp2p::request_response::{ProtocolSupport, RequestResponse, RequestResponseEvent};
use libp2p::swarm::Swarm;
use libp2p::{rendezvous, Multiaddr, NetworkBehaviour};
use log::error;
use responder::Event;
pub use starknet_api::{BlockHeader, BlockNumber};
use sync::{
    BlockHeaderExchangeCodec, BlockHeaderExchangeProtocol, BlockHeaderRequest, BlockHeaderResponse,
};

const PING_DURATION: Duration = Duration::from_secs(60);
const RENDEZVOUS_POINT_ADDRESS: &str = "/ip4/127.0.0.1/tcp/62649";
const RENDEZVOUS_POINT: &str = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";
const NAMESPACE: &str = "rendezvous";
const PROTOCOL_VERSION: &str = "starknet-p2p/1.0.0";

pub struct P2P {}

#[derive(thiserror::Error, Debug)]
pub enum P2PError {
    #[error(transparent)]
    DialError(#[from] libp2p::swarm::DialError),
    #[error("Sync error: {message:?}.")]
    SyncError { message: String },
}

pub async fn new() -> anyhow::Result<(Client, Receiver<Event>, EventLoop)> {
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = id_keys.public().to_peer_id();

    // Build the Swarm, connecting the lower layer transport logic with the
    // higher layer network behaviour logic.
    let swarm = Swarm::new(
        libp2p::development_transport(id_keys.clone()).await?,
        MyBehaviour {
            identify: Identify::new(IdentifyConfig::new(
                PROTOCOL_VERSION.to_string(),
                id_keys.public(),
            )),
            ping: Ping::new(PingConfig::new().with_interval(PING_DURATION).with_keep_alive(true)),
            rendezvous: rendezvous::client::Behaviour::new(id_keys.clone()),
            request_response: RequestResponse::new(
                BlockHeaderExchangeCodec(),
                iter::once((BlockHeaderExchangeProtocol(), ProtocolSupport::Full)),
                Default::default(),
            ),
        },
        peer_id,
    );

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, event_receiver) = mpsc::channel(0);

    Ok((
        Client { sender: command_sender },
        event_receiver,
        EventLoop::new(swarm, command_receiver, event_sender),
    ))
}

#[derive(Debug)]
pub enum MyEvent {
    Identify(Box<IdentifyEvent>),
    Ping(PingEvent),
    Rendezvous(rendezvous::client::Event),
    RequestResponse(RequestResponseEvent<BlockHeaderRequest, BlockHeaderResponse>),
}

impl From<IdentifyEvent> for MyEvent {
    fn from(event: IdentifyEvent) -> Self {
        MyEvent::Identify(Box::new(event))
    }
}

impl From<rendezvous::client::Event> for MyEvent {
    fn from(event: rendezvous::client::Event) -> Self {
        MyEvent::Rendezvous(event)
    }
}

impl From<PingEvent> for MyEvent {
    fn from(event: PingEvent) -> Self {
        MyEvent::Ping(event)
    }
}

impl From<RequestResponseEvent<BlockHeaderRequest, BlockHeaderResponse>> for MyEvent {
    fn from(event: RequestResponseEvent<BlockHeaderRequest, BlockHeaderResponse>) -> Self {
        MyEvent::RequestResponse(event)
    }
}

#[derive(NetworkBehaviour)]
// #[behaviour(event_process = false)]
#[behaviour(out_event = "MyEvent")]
pub struct MyBehaviour {
    identify: Identify,
    ping: Ping,
    rendezvous: rendezvous::client::Behaviour,
    request_response: RequestResponse<BlockHeaderExchangeCodec>,
}
