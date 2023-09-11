#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;

use std::{io, iter};

use futures::channel::mpsc::{unbounded, TrySendError, UnboundedReceiver, UnboundedSender};
use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;
use prost::Message;

use crate::messages::block::{GetBlocks, GetBlocksResponse};
use crate::messages::{read_message, write_message};

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/get_blocks/1.0.0");

/// Substream upgrade protocol for sending data on blocks.
///
/// Receives a request to get a range of blocks and sends a stream of data on the blocks.
pub struct ResponseProtocol;

impl UpgradeInfo for ResponseProtocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream> InboundUpgrade<Stream> for ResponseProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, mut io: Stream, _: Self::Info) -> Self::Future {
        async move {
            read_message::<GetBlocks, _>(&mut io).await?;
            for response in hardcoded_responses() {
                write_message(response, &mut io).await?;
            }
            io.close().await?;
            Ok(())
        }
        .boxed()
    }
}

/// Substream upgrade protocol for requesting data on blocks.
///
/// Sends a request to get a range of blocks and receives a stream of data on the blocks.
#[derive(Debug)]
pub struct OutboundProtocol<Query: Message, Data: Message> {
    query: Query,
    data_sender: UnboundedSender<Data>,
}

impl<Query: Message, Data: Message> OutboundProtocol<Query, Data> {
    pub fn new(query: Query) -> (Self, UnboundedReceiver<Data>) {
        let (data_sender, data_receiver) = unbounded();
        (Self { query, data_sender }, data_receiver)
    }

    #[cfg(test)]
    pub(crate) fn query(&self) -> &Query {
        &self.query
    }

    #[cfg(test)]
    pub(crate) fn data_sender(&self) -> &UnboundedSender<Data> {
        &self.data_sender
    }
}

#[derive(thiserror::Error, Debug)]
pub enum OutboundProtocolError<Data: Message> {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    ResponseSendError(#[from] TrySendError<Data>),
}

impl<Query: Message, Data: Message> UpgradeInfo for OutboundProtocol<Query, Data> {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream, Query: Message + 'static, Data: Message + Default + 'static> OutboundUpgrade<Stream>
    for OutboundProtocol<Query, Data>
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = OutboundProtocolError<Data>;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, mut io: Stream, _: Self::Info) -> Self::Future {
        async move {
            write_message(self.query, &mut io).await?;
            loop {
                let data = read_message::<Data, _>(&mut io).await?;
                // if data.is_fin() {
                //     io.close().await?;
                //     return Ok(());
                // }
                self.data_sender.unbounded_send(data)?;
            }
        }
        .boxed()
    }
}

use crate::messages::block::BlockHeader;
use crate::messages::common::{BlockId, Fin};
use crate::messages::proto::p2p::proto::get_blocks_response::Response;

// TODO(shahak): Remove this and read data from storage instead.
pub fn hardcoded_responses() -> Vec<GetBlocksResponse> {
    vec![
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 1 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 2 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 3 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse { response: Some(Response::Fin(Fin {})) },
    ]
}
