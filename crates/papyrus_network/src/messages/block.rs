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
/// A request to get a range of block headers and state diffs.
pub type GetBlocks = crate::messages::proto::p2p::proto::GetBlocks;
/// A response of a [`GetBlocks`] request.
pub type GetBlocksResponse = crate::messages::proto::p2p::proto::GetBlocksResponse;
/// A request to get signatures on a block.
pub type GetSignatures = crate::messages::proto::p2p::proto::GetSignatures;

impl GetBlocksResponse {
    pub fn is_fin(&self) -> bool {
        self.response.as_ref().map_or(false, |response| {
            matches!(
                response,
                crate::messages::proto::p2p::proto::get_blocks_response::Response::Fin(_)
            )
        })
    }
}
