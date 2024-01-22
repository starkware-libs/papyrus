use futures::Stream;
use starknet_api::block::{BlockHeader, BlockSignature};

use crate::BlockQuery;

pub mod dummy_executor;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct QueryId(pub usize);

pub enum Data {
    BlockHeaderAndSignature { header: BlockHeader, signature: BlockSignature },
    Fin,
}

pub(crate) trait DBExecutor: Stream<Item = (QueryId, Data)> + Unpin {
    // TODO: add writer functionality
    fn register_query(&mut self, query: BlockQuery) -> QueryId;
}
