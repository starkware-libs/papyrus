use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use mockall::mock;
use papyrus_protobuf::consensus::ConsensusMessage;
use starknet_api::block::{BlockHash, BlockNumber};

use crate::types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

/// Define a consensus block which can be used to enable auto mocking Context.
#[derive(Debug, PartialEq, Clone)]
pub struct TestBlock {
    pub content: Vec<u32>,
    pub id: BlockHash,
}

impl ConsensusBlock for TestBlock {
    type ProposalChunk = u32;
    type ProposalIter = std::vec::IntoIter<u32>;

    fn id(&self) -> BlockHash {
        self.id
    }

    fn proposal_iter(&self) -> Self::ProposalIter {
        self.content.clone().into_iter()
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

        async fn validators(&self, height: BlockNumber) -> Vec<ValidatorId>;

        fn proposer(&self, validators: &[ValidatorId], height: BlockNumber) -> ValidatorId;

        async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

        async fn propose(
            &self,
            init: ProposalInit,
            content_receiver: mpsc::Receiver<u32>,
            fin_receiver: oneshot::Receiver<BlockHash>,
        ) -> Result<(), ConsensusError>;
    }
}
