use std::iter;
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, FutureExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::core::Endpoint;
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionHandler,
    ConnectionHandlerEvent,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    Stream,
    SubstreamProtocol,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId, StreamProtocol};

#[derive(Default)]
pub(crate) struct Behaviour {
    stream: Option<Stream>,
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Stream;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler { request_outbound_session: false, stream: None })
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler { request_outbound_session: true, stream: None })
    }

    fn on_swarm_event(&mut self, _event: FromSwarm<'_>) {}

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        stream: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        self.stream = Some(stream);
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        if let Some(stream) = self.stream.take() {
            return Poll::Ready(ToSwarm::GenerateEvent(stream));
        }
        Poll::Pending
    }
}

pub(crate) struct Handler {
    request_outbound_session: bool,
    stream: Option<Stream>,
}

impl ConnectionHandler for Handler {
    type FromBehaviour = ();
    type ToBehaviour = Stream;
    type InboundProtocol = Protocol;
    type OutboundProtocol = Protocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(Protocol, ())
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if self.request_outbound_session {
            self.request_outbound_session = false;
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(Protocol, ()),
            });
        }
        if let Some(stream) = self.stream.take() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(stream));
        }
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {}

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
        match event {
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                info: _,
            }) => self.stream = Some(stream),
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                info: _,
            }) => self.stream = Some(stream),
            _ => {}
        }
    }
}

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/get_stream");

pub(crate) struct Protocol;

impl UpgradeInfo for Protocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl OutboundUpgrade<Stream> for Protocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = Stream;
    type Error = ();
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, stream: Stream, _: Self::Info) -> Self::Future {
        async move { Ok(stream) }.boxed()
    }
}

impl InboundUpgrade<Stream> for Protocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = Stream;
    type Error = ();
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, stream: Stream, _: Self::Info) -> Self::Future {
        async move { Ok(stream) }.boxed()
    }
}
