use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use mockall::mock;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkFelt;
use tokio;

use super::SingleHeightConsensus;
use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    NodeId,
    PeeringConsensusMessage,
    ProposalInit,
};

#[derive(PartialEq, Debug)]
pub struct TestBlock {
    pub inner: Vec<u32>,
    pub id: u64,
}

impl ConsensusBlock for TestBlock {
    type ProposalChunk = u32;
    type ProposalIter = std::vec::IntoIter<u32>;

    fn id(&self) -> BlockHash {
        BlockHash(StarkFelt::try_from(self.id as u128).unwrap())
    }

    fn proposal_iter(&self) -> Self::ProposalIter {
        self.inner.clone().into_iter()
    }
}

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

type Shc = SingleHeightConsensus<TestBlock>;
type PeeringMessage = PeeringConsensusMessage<u32>;

struct TestFields {
    pub context: MockTestContext,
    pub shc_to_peering_sender: mpsc::Sender<PeeringConsensusMessage<u32>>,
    pub shc_to_peering_receiver: mpsc::Receiver<PeeringConsensusMessage<u32>>,
    pub peering_to_shc_sender: mpsc::Sender<PeeringConsensusMessage<u32>>,
    pub peering_to_shc_receiver: mpsc::Receiver<PeeringConsensusMessage<u32>>,
}

impl TestFields {
    async fn new_shc(
        self,
        height: BlockNumber,
        id: NodeId,
    ) -> (
        Shc,
        mpsc::Receiver<PeeringConsensusMessage<u32>>,
        mpsc::Sender<PeeringConsensusMessage<u32>>,
    ) {
        let shc = Shc::new(
            height,
            Arc::new(self.context),
            id,
            self.shc_to_peering_sender,
            self.peering_to_shc_receiver,
        )
        .await;
        (shc, self.shc_to_peering_receiver, self.peering_to_shc_sender)
    }
}

fn setup() -> TestFields {
    let (shc_to_peering_sender, shc_to_peering_receiver) = mpsc::channel(1);
    let (peering_to_shc_sender, peering_to_shc_receiver) = mpsc::channel(1);
    let context = MockTestContext::new();
    TestFields {
        context,
        shc_to_peering_sender,
        shc_to_peering_receiver,
        peering_to_shc_sender,
        peering_to_shc_receiver,
    }
}

#[tokio::test]
async fn propose() {
    let mut test_fields = setup();
    let id: NodeId = 1;
    // Set expectations for how the test should run:
    test_fields.context.expect_validators().returning(move |_| vec![id, 2, 3, 4]);
    test_fields.context.expect_proposer().returning(move |_, _| id);
    test_fields.context.expect_build_proposal().returning(|_| {
        // SHC doesn't actually handle the content, so ignore for unit tests.
        let (_, content_receiver) = mpsc::channel(1);
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(TestBlock { inner: vec![100, 101], id: 0 }).unwrap();
        (content_receiver, block_receiver)
    });

    // Creation calls to `context.validators`.
    let (shc, mut shc_to_peering_receiver, _) = test_fields.new_shc(BlockNumber(0), id).await;

    // This calls to `context.proposer` and `context.build_proposal`.
    let block = shc.run().await;
    assert_eq!(block, TestBlock { inner: vec![100, 101], id: 0 });

    // Check what was sent to peering. We don't check the content stream as that is filled by
    // ConsensusContext, not SHC.
    let PeeringMessage::Proposal((init, _, block_hash_receiver)) =
        shc_to_peering_receiver.next().await.unwrap();
    assert_eq!(init, ProposalInit { height: BlockNumber(0), proposer: id });
    assert_eq!(
        block_hash_receiver.await.unwrap(),
        BlockHash(StarkFelt::try_from(0 as u128).unwrap())
    );
}
