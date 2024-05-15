use crate::types::{ConsensusBlock, Context};

// This should cause compilation to fail if the traits are not object safe.
#[test]
fn check_object_safety() {
    // Arbitrarily chosen types for testing.
    type _ProposalIter = std::slice::Iter<'static, u32>;
    type _Blk = Box<dyn ConsensusBlock<ProposalChunk = u32, ProposalIter = _ProposalIter>>;

    fn _check_consensus_block() -> _Blk {
        todo!()
    }

    fn _check_context() -> Box<dyn Context<Block = _Blk>> {
        todo!()
    }
}
