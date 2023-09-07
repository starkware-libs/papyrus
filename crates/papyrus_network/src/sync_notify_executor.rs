use futures::channel::mpsc::UnboundedSender;
use futures::channel::oneshot;
#[cfg(test)]
use mockall::automock;

use crate::BlocksRange;

#[derive(thiserror::Error)]
pub enum WriterError {}

pub struct WriterCommunication<Response> {
    pub result_sender: UnboundedSender<Response>,
    pub is_finished: oneshot::Receiver<Result<(), WriterError>>,
}

#[cfg_attr(test, automock)]
pub trait WriterExecutor<Response> {
    fn start_writing(&self, blocks_range: BlocksRange) -> WriterCommunication<Response>;
}
