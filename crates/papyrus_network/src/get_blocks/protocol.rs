#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;

use std::{io, iter};

use futures::channel::mpsc::{unbounded, TrySendError, UnboundedReceiver, UnboundedSender};
use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt, StreamExt};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::swarm::StreamProtocol;

use crate::messages::block::{GetBlocks, GetBlocksResponse};
use crate::messages::{read_message, write_message};

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/get_blocks/1.0.0");

/// Substream upgrade protocol for sending data on blocks.
///
/// Receives a request to get a range of blocks and sends a stream of data on the blocks.
pub struct InboundProtocol {
    request_relay_sender: UnboundedSender<GetBlocks>,
    response_relay_receiver: UnboundedReceiver<Option<GetBlocksResponse>>,
}

impl InboundProtocol {
    pub fn new()
    -> (Self, (UnboundedReceiver<GetBlocks>, UnboundedSender<Option<GetBlocksResponse>>)) {
        let (request_relay_sender, request_relay_receiver) = unbounded();
        let (response_relay_sender, response_relay_receiver) = unbounded();
        (
            Self { request_relay_sender, response_relay_receiver },
            (request_relay_receiver, response_relay_sender),
        )
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InboundProtocolError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    RequestSendError(#[from] TrySendError<GetBlocks>),
}

impl UpgradeInfo for InboundProtocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream> InboundUpgrade<Stream> for InboundProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = InboundProtocolError;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(mut self, mut io: Stream, _: Self::Info) -> Self::Future {
        Box::pin(
            async move {
                if let Ok(get_blocks_msg) = read_message::<GetBlocks, _>(&mut io).await {
                    self.request_relay_sender.unbounded_send(get_blocks_msg)?;
                }
                let mut expect_end_of_stream = false;
                loop {
                    match self.response_relay_receiver.next().await {
                        Some(response) => match response {
                            Some(res) => write_message(res, &mut io).await?,
                            None => {
                                expect_end_of_stream = true;
                                write_message(
                                    GetBlocksResponse { response: Some(Response::Fin(Fin {})) },
                                    &mut io,
                                )
                                .await?;
                            }
                        },
                        None => {
                            if expect_end_of_stream {
                                return Ok(());
                            }
                            return Err(InboundProtocolError::IOError(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "Unexpected end of stream",
                            )));
                        }
                    };
                }
            }
            .boxed(),
        )
    }
}

/// Substream upgrade protocol for requesting data on blocks.
///
/// Sends a request to get a range of blocks and receives a stream of data on the blocks.
#[derive(Debug)]
pub struct OutboundProtocol {
    request: GetBlocks,
    responses_sender: UnboundedSender<GetBlocksResponse>,
}

impl OutboundProtocol {
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
pub enum OutboundProtocolError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    ResponseSendError(#[from] TrySendError<GetBlocksResponse>),
}

impl UpgradeInfo for OutboundProtocol {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(PROTOCOL_NAME)
    }
}

impl<Stream> OutboundUpgrade<Stream> for OutboundProtocol
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = ();
    type Error = OutboundProtocolError;
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
