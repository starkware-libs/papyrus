use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::FutureExt;
use libp2p::swarm::Stream;
use replace_with::replace_with_or_abort;

use super::super::DataBound;
use crate::messages::write_message;

pub(super) struct InboundSession<Data: DataBound> {
    pending_messages: VecDeque<Data>,
    current_task: WriteMessageTask,
}

enum WriteMessageTask {
    Waiting(Stream),
    Running(BoxFuture<'static, Result<Stream, io::Error>>),
}

impl<Data: DataBound> InboundSession<Data> {
    #[allow(dead_code)]
    // TODO(shahak) remove allow dead code.
    pub fn new(stream: Stream) -> Self {
        Self {
            pending_messages: Default::default(),
            current_task: WriteMessageTask::Waiting(stream),
        }
    }

    #[allow(dead_code)]
    // TODO(shahak) remove allow dead code.
    pub fn add_message_to_queue(&mut self, data: Data) {
        self.pending_messages.push_back(data);
    }

    #[allow(dead_code)]
    // TODO(shahak) remove allow dead code.
    pub fn is_waiting(&self) -> bool {
        matches!(self.current_task, WriteMessageTask::Waiting(_))
            && self.pending_messages.is_empty()
    }

    fn handle_waiting(&mut self, cx: &mut Context<'_>) -> Option<io::Error> {
        if let Some(data) = self.pending_messages.pop_front() {
            replace_with_or_abort(&mut self.current_task, |current_task| {
                let WriteMessageTask::Waiting(mut stream) = current_task else {
                    panic!("Called handle_waiting while running.");
                };
                WriteMessageTask::Running(
                    async move {
                        write_message(data, &mut stream).await?;
                        Ok(stream)
                    }
                    .boxed(),
                )
            });
            return self.handle_running(cx);
        }
        None
    }

    fn handle_running(&mut self, cx: &mut Context<'_>) -> Option<io::Error> {
        let WriteMessageTask::Running(fut) = &mut self.current_task else {
            panic!("Called handle_running while waiting.");
        };
        match fut.poll_unpin(cx) {
            Poll::Pending => None,
            Poll::Ready(Ok(stream)) => {
                self.current_task = WriteMessageTask::Waiting(stream);
                self.handle_waiting(cx)
            }
            Poll::Ready(Err(io_error)) => Some(io_error),
        }
    }
}

impl<Data: DataBound> Future for InboundSession<Data> {
    type Output = io::Error;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let unpinned_self = Pin::into_inner(self);
        let result = match &mut unpinned_self.current_task {
            WriteMessageTask::Running(_) => unpinned_self.handle_running(cx),
            WriteMessageTask::Waiting(_) => unpinned_self.handle_waiting(cx),
        };
        match result {
            Some(error) => Poll::Ready(error),
            None => Poll::Pending,
        }
    }
}
