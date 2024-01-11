use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll, Waker};

use futures::future::BoxFuture;
use futures::io::ReadHalf;
use futures::stream::Stream as StreamTrait;
use futures::{AsyncReadExt, AsyncWriteExt, FutureExt};
use libp2p::swarm::Stream;

use super::super::DataBound;
use crate::messages::read_message;

pub(super) struct OutboundSession<Data: DataBound> {
    read_task: BoxFuture<'static, Result<(Data, ReadHalf<Stream>), FinishReason>>,
    close_task: BoxFuture<'static, FinishReason>,
    should_close: bool,
    finished: bool,
    wakers: Vec<Waker>,
}

pub(super) enum FinishReason {
    Error(io::Error),
    Closed,
    OtherPeerClosed,
}

impl<Data: DataBound> OutboundSession<Data> {
    pub fn new(stream: Stream) -> Self {
        let (read_half, mut write_half) = stream.split();
        Self {
            read_task: Self::read_data(read_half).boxed(),
            close_task: async move {
                match write_half.close().await {
                    Ok(()) => FinishReason::Closed,
                    Err(io_error) => FinishReason::Error(io_error),
                }
            }
            .boxed(),
            should_close: false,
            finished: false,
            wakers: vec![],
        }
    }

    pub fn start_closing(&mut self) {
        self.should_close = true;
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }

    async fn read_data(
        mut stream: ReadHalf<Stream>,
    ) -> Result<(Data, ReadHalf<Stream>), FinishReason> {
        match read_message::<Data, _>(&mut stream).await {
            Ok(Some(data)) => Ok((data, stream)),
            Ok(None) => Err(FinishReason::OtherPeerClosed),
            Err(io_error) => Err(FinishReason::Error(io_error)),
        }
    }
}

impl<Data: DataBound> StreamTrait for OutboundSession<Data> {
    type Item = Result<Data, FinishReason>;

    // It is guaranteed that the stream will finish after and only after returning a FinishReason.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        if unpinned_self.finished {
            return Poll::Ready(None);
        }
        if unpinned_self.should_close {
            let finish_reason = ready!(unpinned_self.close_task.poll_unpin(cx));
            unpinned_self.finished = true;
            return Poll::Ready(Some(Err(finish_reason)));
        }

        let Poll::Ready(result_with_stream) = unpinned_self.read_task.poll_unpin(cx) else {
            unpinned_self.wakers.push(cx.waker().clone());
            return Poll::Pending;
        };

        let result = result_with_stream.map(|(data, stream)| {
            unpinned_self.read_task = Self::read_data(stream).boxed();
            data
        });
        if result.is_err() {
            unpinned_self.finished = true;
        }
        Poll::Ready(Some(result))
    }
}
