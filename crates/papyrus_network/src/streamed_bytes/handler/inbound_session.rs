use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll, Waker};

use futures::future::BoxFuture;
use futures::io::WriteHalf;
use futures::{AsyncWriteExt, FutureExt};
use libp2p::swarm::Stream;
use replace_with::replace_with_or_abort;

use super::super::messages::write_message;
use super::super::Bytes;

pub(super) struct InboundSession {
    pending_messages: VecDeque<Bytes>,
    current_task: WriteMessageTask,
    wakers_waiting_for_new_message: Vec<Waker>,
}

enum FinishReason {
    Error(io::Error),
    Closed,
}

enum WriteMessageTask {
    Waiting(WriteHalf<Stream>),
    Running(BoxFuture<'static, Result<WriteHalf<Stream>, io::Error>>),
    Closing(BoxFuture<'static, Result<(), io::Error>>),
}

impl InboundSession {
    pub fn new(write_stream: WriteHalf<Stream>) -> Self {
        Self {
            pending_messages: Default::default(),
            current_task: WriteMessageTask::Waiting(write_stream),
            wakers_waiting_for_new_message: Default::default(),
        }
    }

    pub fn add_message_to_queue(&mut self, data: Bytes) {
        self.pending_messages.push_back(data);
        for waker in self.wakers_waiting_for_new_message.drain(..) {
            waker.wake();
        }
    }

    pub fn is_waiting(&self) -> bool {
        matches!(self.current_task, WriteMessageTask::Waiting(_))
            && self.pending_messages.is_empty()
    }

    pub fn start_closing(&mut self) {
        replace_with_or_abort(&mut self.current_task, |current_task| {
            let WriteMessageTask::Waiting(mut write_stream) = current_task else {
                panic!("Called start_closing while not waiting.");
            };
            WriteMessageTask::Closing(async move { write_stream.close().await }.boxed())
        })
    }

    fn handle_waiting(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(data) = self.pending_messages.pop_front() {
            replace_with_or_abort(&mut self.current_task, |current_task| {
                let WriteMessageTask::Waiting(mut write_stream) = current_task else {
                    panic!("Called handle_waiting while not waiting.");
                };
                WriteMessageTask::Running(
                    async move {
                        write_message(&data, &mut write_stream).await?;
                        Ok(write_stream)
                    }
                    .boxed(),
                )
            });
            Poll::Ready(())
        } else {
            self.wakers_waiting_for_new_message.push(cx.waker().clone());
            Poll::Pending
        }
    }

    fn handle_running(&mut self, cx: &mut Context<'_>) -> Poll<Option<FinishReason>> {
        let WriteMessageTask::Running(fut) = &mut self.current_task else {
            panic!("Called handle_running while not running.");
        };
        fut.poll_unpin(cx).map(|result| match result {
            Ok(write_stream) => {
                self.current_task = WriteMessageTask::Waiting(write_stream);
                None
            }
            Err(io_error) => Some(FinishReason::Error(io_error)),
        })
    }

    fn handle_closing(&mut self, cx: &mut Context<'_>) -> Poll<FinishReason> {
        let WriteMessageTask::Closing(fut) = &mut self.current_task else {
            panic!("Called handle_closing while not closing.");
        };
        fut.poll_unpin(cx).map(|result| match result {
            Ok(()) => FinishReason::Closed,
            Err(io_error) => FinishReason::Error(io_error),
        })
    }
}

impl Future for InboundSession {
    type Output = Result<(), io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let unpinned_self = Pin::into_inner(self);
        let finish_reason = loop {
            match &mut unpinned_self.current_task {
                WriteMessageTask::Running(_) => {
                    if let Some(finish_reason) = ready!(unpinned_self.handle_running(cx)) {
                        break finish_reason;
                    }
                }
                WriteMessageTask::Waiting(_) => {
                    ready!(unpinned_self.handle_waiting(cx));
                }
                WriteMessageTask::Closing(_) => {
                    break ready!(unpinned_self.handle_closing(cx));
                }
            }
        };
        match finish_reason {
            FinishReason::Error(io_error) => Poll::Ready(Err(io_error)),
            FinishReason::Closed => Poll::Ready(Ok(())),
        }
    }
}
