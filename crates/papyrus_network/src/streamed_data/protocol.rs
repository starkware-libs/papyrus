#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;

use std::marker::PhantomData;
use std::{io, iter};

use futures::future::BoxFuture;
use futures::io::{ReadHalf, WriteHalf};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, FutureExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;
use prost::Message;

use crate::messages::{read_message_without_length_prefix, write_message_without_length_prefix};

/// Substream upgrade protocol for sending data on blocks.
///
/// Receives a request to get a range of blocks and sends a stream of data on the blocks.
pub struct InboundProtocol<Query: Message + Default> {
    phantom: PhantomData<Query>,
    protocol_name: StreamProtocol,
}

impl<Query: Message + Default> InboundProtocol<Query> {
    pub fn new(protocol_name: StreamProtocol) -> Self {
        Self { protocol_name, phantom: PhantomData }
    }
}

impl<Query: Message + Default> UpgradeInfo for InboundProtocol<Query> {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(self.protocol_name.clone())
    }
}

impl<Stream, Query> InboundUpgrade<Stream> for InboundProtocol<Query>
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    Query: Message + Default,
{
    type Output = (Query, WriteHalf<Stream>);
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, stream: Stream, _: Self::Info) -> Self::Future {
        async move {
            let (read_half, write_half) = stream.split();
            let request = read_message_without_length_prefix::<Query, _>(read_half).await?;
            Ok((request, write_half))
        }
        .boxed()
    }
}

/// Substream upgrade protocol for requesting data on blocks.
///
/// Sends a request to get a range of blocks and receives a stream of data on the blocks.
#[derive(Debug)]
pub struct OutboundProtocol<Query: Message + Default> {
    pub query: Query,
    // TODO(shahak): Think of a way to allow multiple protocols with different Query type for
    // each.
    pub protocol_name: StreamProtocol,
}

impl<Query: Message + Default> UpgradeInfo for OutboundProtocol<Query> {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(self.protocol_name.clone())
    }
}

impl<Stream, Query: Message + Default + 'static> OutboundUpgrade<Stream> for OutboundProtocol<Query>
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ReadHalf<Stream>;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, stream: Stream, _: Self::Info) -> Self::Future {
        async move {
            let (read_half, write_half) = stream.split();
            write_message_without_length_prefix(self.query, write_half).await?;
            Ok(read_half)
        }
        .boxed()
    }
}
