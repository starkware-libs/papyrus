use std::ops::Range;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use mockall::mock;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkFelt;

use crate::types::{ConsensusBlock, ConsensusContext, NodeId};

pub type TestBlock = u32;

impl ConsensusBlock for TestBlock {
    type ProposalChunk = u32;
    type ProposalIter = Range<u32>;

    fn id(&self) -> BlockHash {
        BlockHash(StarkFelt::try_from(*self as u128).unwrap())
    }

    fn proposal_iter(&self) -> Self::ProposalIter {
        0..*self
    }
}

// TODO(matan): When QSelf is supported, switch to automocking `ConsensusContext`.
mock! {
    pub TestContext {}

    #[async_trait]
    impl ConsensusContext for TestContext {
        type Block = TestBlock;

        async fn build_proposal(&self, height: BlockNumber) -> (
            mpsc::Receiver<u32>,
            oneshot::Receiver<TestBlock>
        );
        async fn validate_proposal(
            &self,
            height: BlockNumber,
            content: mpsc::Receiver<u32>
        ) -> oneshot::Receiver<TestBlock>;
        async fn validators(&self, height: BlockNumber) -> Vec<NodeId>;
        fn proposer(&self, validators: &Vec<NodeId>, height: BlockNumber) -> NodeId;
    }
}
