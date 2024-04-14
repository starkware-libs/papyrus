use std::task::{Context, Poll};
use std::{io, iter};

use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::handler::ConnectionEvent;
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, StreamProtocol, SubstreamProtocol};

// TODO(shahak): Open a request-feature issue to libp2p.
/// A handler that can't open any substreams.
pub struct NullHandler;

impl ConnectionHandler for NullHandler {
    type FromBehaviour = ();
    type ToBehaviour = ();
    type InboundProtocol = NullProtocol;
    type OutboundProtocol = NullProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    // Required methods
    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(NullProtocol, ())
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {}

    fn on_connection_event(
        &mut self,
        _event: ConnectionEvent<
            '_,
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
    }
}

pub struct NullProtocol;

impl UpgradeInfo for NullProtocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Empty<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::empty()
    }
}

impl<Stream> InboundUpgrade<Stream> for NullProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, _stream: Stream, _protocol_name: Self::Info) -> Self::Future {
        panic!("A NullProtocol's negotiation didn't fail");
    }
}

impl<Stream> OutboundUpgrade<Stream> for NullProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, _stream: Stream, _protocol_name: Self::Info) -> Self::Future {
        panic!("A NullProtocol's negotiation didn't fail");
    }
}
