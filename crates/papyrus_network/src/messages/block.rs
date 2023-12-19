// TODO(shahak): Think of a way to auto-generate these.
/// Signatures from the Starknet feeder gateway that show this block and state diffs are part of
/// Starknet
pub type Signatures = crate::messages::proto::p2p::proto::Signatures;
/// Header of a block. Contains hashes for the proof and state diffs and the rest of the block
/// data.
pub type BlockHeader = crate::messages::proto::p2p::proto::BlockHeader;
/// A proof for the Kth block behind the current block.
pub type BlockProof = crate::messages::proto::p2p::proto::BlockProof;
/// A message announcing that a new block was created.
pub type NewBlock = crate::messages::proto::p2p::proto::NewBlock;
/// A request to get a range of block headers.
pub type BlockHeadersRequest = crate::messages::proto::p2p::proto::BlockHeadersRequest;
/// A response of a [`BlockHeadersResponse`] request.
pub type BlockHeadersResponse = crate::messages::proto::p2p::proto::BlockHeadersResponse;

pub type BlockHeadersResponsePart = crate::messages::proto::p2p::proto::BlockHeadersResponsePart;

impl BlockHeadersResponse {
    pub fn is_fin(&self) -> bool {
        self.part.last().map_or(false, |response| {
            matches!(
                response,
                crate::messages::proto::p2p::proto::BlockHeadersResponsePart{header_message: Some(crate::messages::proto::p2p::proto::block_headers_response_part::HeaderMessage::Fin(_))}
            )
        })
    }
}
