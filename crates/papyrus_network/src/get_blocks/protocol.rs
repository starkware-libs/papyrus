#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;

use std::{io, iter};

use futures::channel::mpsc::{unbounded, TrySendError, UnboundedReceiver, UnboundedSender};
use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;

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
pub struct RequestProtocol {
    request: GetBlocks,
    responses_sender: UnboundedSender<GetBlocksResponse>,
}

impl RequestProtocol {
    pub fn new(request: GetBlocks) -> (Self, UnboundedReceiver<GetBlocksResponse>) {
        let (responses_sender, responses_receiver) = unbounded();
        (Self { request, responses_sender }, responses_receiver)
    }

    #[cfg(test)]
    pub(crate) fn request(&self) -> &GetBlocks {
        &self.request
    }

    #[cfg(test)]
    pub(crate) fn responses_sender(&self) -> &UnboundedSender<GetBlocksResponse> {
        &self.responses_sender
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequestProtocolError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    ResponseSendError(#[from] TrySendError<GetBlocksResponse>),
}

impl UpgradeInfo for RequestProtocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream> OutboundUpgrade<Stream> for RequestProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = RequestProtocolError;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(self, mut io: Stream, _: Self::Info) -> Self::Future {
        async move {
            write_message(self.request, &mut io).await?;
            loop {
                let response = read_message::<GetBlocksResponse, _>(&mut io).await?;
                if response.is_fin() {
                    io.close().await?;
                    return Ok(());
                }
                self.responses_sender.unbounded_send(response)?;
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
