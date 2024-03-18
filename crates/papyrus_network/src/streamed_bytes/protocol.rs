#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;

use std::{io, iter};

use futures::future::BoxFuture;
use futures::io::{ReadHalf, WriteHalf};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, FutureExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;

use super::messages::{read_message_without_length_prefix, write_message_without_length_prefix};
use super::Bytes;

pub struct InboundProtocol {
    supported_protocols: Vec<StreamProtocol>,
}

impl InboundProtocol {
    pub fn new(supported_protocols: Vec<StreamProtocol>) -> Self {
        Self { supported_protocols }
    }
}

impl UpgradeInfo for InboundProtocol {
    type Info = StreamProtocol;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.supported_protocols.clone()
    }
}

impl<Stream> InboundUpgrade<Stream> for InboundProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (Bytes, WriteHalf<Stream>, StreamProtocol);
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, stream: Stream, protocol_name: Self::Info) -> Self::Future {
        async move {
            let (read_half, write_half) = stream.split();
            let request = read_message_without_length_prefix(read_half).await?;
            Ok((request, write_half, protocol_name))
        }
        .boxed()
    }
}

#[derive(Debug)]
pub struct OutboundProtocol {
    pub query: Bytes,
    pub protocol_name: StreamProtocol,
}

impl UpgradeInfo for OutboundProtocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(self.protocol_name.clone())
    }
}

impl<Stream> OutboundUpgrade<Stream> for OutboundProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ReadHalf<Stream>;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, stream: Stream, _: Self::Info) -> Self::Future {
        async move {
            let (read_half, write_half) = stream.split();
            write_message_without_length_prefix(&self.query, write_half).await?;
            Ok(read_half)
        }
        .boxed()
    }
}
