use futures::channel::mpsc::UnboundedSender;
use prost::Message;

use crate::BlocksRange;

pub trait ReaderExecutor<Response: Message> {
    fn spawn_reader(&self, blocks_range: BlocksRange) -> UnboundedSender<Response>;
}
