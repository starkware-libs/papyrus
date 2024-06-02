use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::sink::SinkExt;
use futures::StreamExt;
use mockall::mock;
use starknet_api::block::{BlockHash, BlockNumber};

use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    NetworkSender,
    ProposalInit,
    ValidatorId,
};

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

/// What is sent out to tests from the `TestNetworkSender`.
pub enum TestMessages {
    Proposal((ProposalInit, TestBlock)),
}

#[derive(Debug, Clone)]
pub struct TestNetworkSender {
    pub sender: mpsc::Sender<TestMessages>,
}

#[async_trait]
impl NetworkSender for TestNetworkSender {
    type ProposalChunk = u32;

    async fn propose(
        &mut self,
        init: ProposalInit,
        content_receiver: mpsc::Receiver<Self::ProposalChunk>,
        fin_receiver: oneshot::Receiver<BlockHash>,
    ) -> Result<(), ConsensusError> {
        let mut sender = self.sender.clone();
        let _ = tokio::spawn(async move {
            let content = content_receiver.collect::<Vec<u32>>().await;
            let id = fin_receiver.await.unwrap();
            let block = TestBlock { content, id };
            sender.send(TestMessages::Proposal((init, block))).await.unwrap();
        });
        Ok(())
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
        fn proposer(&self, validators: &Vec<ValidatorId>, height: BlockNumber) -> ValidatorId;
    }
}
