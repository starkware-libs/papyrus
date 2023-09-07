#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;

use std::marker::PhantomData;
use std::{io, iter};

use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, FutureExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;
use prost::Message;

use crate::messages::{read_message, write_message};

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/get_blocks/1.0.0");

/// Substream upgrade protocol for sending data on blocks.
///
/// Receives a request to get a range of blocks and sends a stream of data on the blocks.
pub struct InboundProtocol<Query: Message + Default> {
    phantom: PhantomData<Query>,
}

impl<Query: Message + Default> InboundProtocol<Query> {
    pub fn new() -> Self {
        Self { phantom: PhantomData }
    }
}

impl<Query: Message + Default> UpgradeInfo for InboundProtocol<Query> {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream, Query> InboundUpgrade<Stream> for InboundProtocol<Query>
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    Query: Message + Default,
{
    type Output = (Query, Stream);
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, mut stream: Stream, _: Self::Info) -> Self::Future {
        async move {
            let request = read_message::<Query, _>(&mut stream).await?;
            Ok((request, stream))
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
}

impl<Query: Message + Default> UpgradeInfo for OutboundProtocol<Query> {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream, Query: Message + Default + 'static> OutboundUpgrade<Stream> for OutboundProtocol<Query>
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = Stream;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, mut stream: Stream, _: Self::Info) -> Self::Future {
        async move {
            write_message(self.query, &mut stream).await?;
            Ok(stream)
        }
        .boxed()
    }
}
