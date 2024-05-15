use crate::types::{ConsensusBlock, Context};

// Arbitrarily chosen types for testing.
type ProposalIter = std::slice::Iter<'static, u32>;
type Blk = Box<dyn ConsensusBlock<StreamT = u32, ProposalIter = ProposalIter>>;

fn check_consensus_block() -> Blk {
    todo!()
}

fn check_context() -> Box<dyn Context<BlockT = Blk>> {
    todo!()
}

// Compile time test.
#[test]
#[should_panic]
fn _check_object_safety() {
    let _blk = check_consensus_block();
    let _ctx = check_context();
}
