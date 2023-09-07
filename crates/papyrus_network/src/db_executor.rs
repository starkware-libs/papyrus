use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::oneshot;
#[cfg(test)]
use mockall::automock;

use crate::BlockQuery;

#[derive(thiserror::Error, Debug)]
pub enum ReaderError {}

#[derive(thiserror::Error, Debug)]
pub enum BlockError {}

pub struct ReaderCommunication<Response> {
    pub result_receiver: UnboundedReceiver<Response>,
    pub error_receiver: UnboundedReceiver<BlockError>,
    pub is_finished: oneshot::Receiver<Result<(), ReaderError>>,
}

#[cfg_attr(test, automock)]
pub trait ReaderExecutor<Response> {
    fn start_reading(&self, blocks_range: BlockQuery) -> ReaderCommunication<Response>;
}
