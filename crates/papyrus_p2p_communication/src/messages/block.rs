pub mod proto {
    pub mod p2p {
        pub mod proto {
            include!(concat!(env!("OUT_DIR"), "/_.rs"));
        }
    }
}

/// Signatures from the Starknet feeder gateway that show this block and state diffs are part of
/// Starknet
pub type Signatures = proto::p2p::proto::Signatures;
/// Header of a block. Contains hashes for the proof and state diffs and the rest of the block
/// data.
pub type BlockHeader = proto::p2p::proto::BlockHeader;
/// A proof for the Kth block behind the current block.
pub type BlockProof = proto::p2p::proto::BlockProof;
/// A message announcing that a new block was created.
pub type NewBlock = proto::p2p::proto::NewBlock;
/// A request to get a range of block headers and state diffs.
pub type GetBlocks = proto::p2p::proto::GetBlocks;
/// A request to get signatures on a block.
pub type GetSignatures = proto::p2p::proto::GetSignatures;
