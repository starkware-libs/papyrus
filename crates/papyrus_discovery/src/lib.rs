use std::collections::VecDeque;
use std::task::{Context, Poll};
use std::thread::sleep;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::prelude::{AsyncRead, AsyncWrite};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, ProtocolName, UpgradeInfo};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::handler::ConnectionEvent;
use libp2p::swarm::{
    ConnectionHandler, ConnectionHandlerEvent, ConnectionId, FromSwarm, KeepAlive,
    NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters, SubstreamProtocol,
};
use libp2p::PeerId;

pub struct DiscoveryBehaviour {
    last_request_sent: Instant,
    dest: PeerId,
    dialed: bool,
    established_connection: bool,
    next_request_id: u64,
}

pub enum DiscoveryMessage {
    Request(u64),
    Response(u64),
}

impl DiscoveryBehaviour {
    pub fn new(dest: PeerId) -> DiscoveryBehaviour {
        DiscoveryBehaviour {
            last_request_sent: Instant::now(),
            dest,
            dialed: false,
            established_connection: false,
            next_request_id: 0,
        }
    }
}

impl NetworkBehaviour for DiscoveryBehaviour {
    type ConnectionHandler = DiscoveryHandler;
    type OutEvent = ();

    fn on_swarm_event(&mut self, event: FromSwarm<'_, Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(_) => {
                self.established_connection = true;
            }
            FromSwarm::ConnectionClosed(_) => {
                self.established_connection = false;
                self.dialed = false;
            }
            FromSwarm::DialFailure(_) => {
                self.established_connection = false;
                self.dialed = false;
            }
            _ => {}
        };
    }

    fn poll(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, u64>> {
        if !self.dialed {
            self.dialed = true;
            return NetworkBehaviourAction::Dial(DialOpts::peer_id(self.dest));
        }
        if !self.established_connection || self.last_request_sent.elapsed().as_secs() < 5 {
            return Poll::Pending;
        }
        self.last_request_sent = Instant::now();
        self.next_request_id += 1;
        Poll::Ready(NetworkBehaviourAction::NotifyHandler(
            self.dest,
            NotifyHandler::Any,
            DiscoveryMessage::Request(self.next_request_id - 1),
        ));
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: (),
    ) {
    }
}

pub struct DiscoveryHandler {
    inbound_protocol: SubstreamProtocol<DiscoveryInboundProtocol, ()>,
    pending_requests: VecDeque<u64>,
}

impl DiscoveryHandler {
    pub fn new() -> DiscoveryHandler {
        return DiscoveryHandler {
            inbound_protocol: SubstreamProtocol::new(DiscoveryInboundProtocol {}),
            pending_requests: vec![[]],
        };
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DiscoveryError {}

impl ConnectionHandler for DiscoveryHandler {
    type InboundProtocol = DiscoveryInboundProtocol;
    type OutboundProtocol = DiscoveryOutboundProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();
    type InEvent = u64;
    type OutEvent = ();
    type Error = DiscoveryError;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        return self.listen_protocol;
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        KeepAlive::Yes;
    }

    fn on_behaviour_event(&mut self, request: u64) {
        self.pending_requests.push_back(request);
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if let Some(request) = self.pending_requests.pop_front() {
            Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest(SubstreamProtocol::new(
                DiscoveryOutboundProtocol { request },
                (),
            )));
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            '_,
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
    }
}

#[derive(Clone, Debug)]
pub struct ProtocolId {}

pub struct DiscoveryOutboundProtocol {
    pub request: u64,
}

impl ProtocolName for ProtocolId {
    fn protocol_name(&self) -> &[u8] {
        "discovery".as_bytes()
    }
}

impl UpgradeInfo for DiscoveryOutboundProtocol {
    type Info = ProtocolId;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![ProtocolId {}]
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for DiscoveryOutboundProtocol
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = DiscoveryError;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, socket: TSocket, info: Self::Info) -> Self::Future {
        async {
            print!("Sending {}", self.request);
            socket.send(DiscoveryMessage::Request(self.request)).await?;
            socket.close().await?;
            Ok(socket)
        }
        .boxed()
    }
}

pub struct DiscoveryInboundProtocol {}

impl UpgradeInfo for DiscoveryInboundProtocol {
    type Info = ProtocolId;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        vec![ProtocolId {}]
    }
}

impl<TSocket> InboundUpgrade<TSocket> for DiscoveryInboundProtocol
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = DiscoveryError;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;
    fn upgrade_inbound(self, socket: TSocket, message: DiscoveryMessage) -> Self::Future {
        async {
            match message {
                DiscoveryMessage::Request(x) => {
                    print!("Received {}", x);
                    print!("Processing {}", x);
                    sleep(Duration::from_secs(1));
                    print!("Finished processing {}. Now sending response", x);
                    socket.send(DiscoveryMessage::Response { x }).await?;
                }
                DiscoveryMessage::Response(x) => {
                    print!("Received response on {}", x);
                }
            }
            socket.close().await?;
            Ok()
        }
        .boxed()
    }
}
