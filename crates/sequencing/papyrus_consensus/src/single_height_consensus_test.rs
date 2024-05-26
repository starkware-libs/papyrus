use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use starknet_api::block::BlockNumber;
use tokio;

use super::SingleHeightConsensus;
use crate::test_utils::{MockTestContext, TestBlock};
use crate::types::{ConsensusBlock, NodeId, PeeringConsensusMessage, ProposalInit};

type Shc = SingleHeightConsensus<TestBlock>;
type ProposalChunk = <TestBlock as ConsensusBlock>::ProposalChunk;
type PeeringMessage = PeeringConsensusMessage<ProposalChunk>;

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
    let node_id: NodeId = 1;
    let block: TestBlock = 2;
    // Set expectations for how the test should run:
    test_fields.context.expect_validators().returning(move |_| vec![node_id, 2, 3, 4]);
    test_fields.context.expect_proposer().returning(move |_, _| node_id);
    test_fields.context.expect_build_proposal().returning(move |_| {
        // SHC doesn't actually handle the content, so ignore for unit tests.
        let (_, content_receiver) = mpsc::channel(1);
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(block).unwrap();
        (content_receiver, block_receiver)
    });

    // Creation calls to `context.validators`.
    let (shc, mut shc_to_peering_receiver, _) = test_fields.new_shc(BlockNumber(0), node_id).await;

    // This calls to `context.proposer` and `context.build_proposal`.
    assert_eq!(shc.run().await, block);

    // Check what was sent to peering. We don't check the content stream as that is filled by
    // ConsensusContext, not SHC.
    let PeeringMessage::Proposal((init, _, block_id_receiver)) =
        shc_to_peering_receiver.next().await.unwrap();
    assert_eq!(init, ProposalInit { height: BlockNumber(0), proposer: node_id });
    assert_eq!(block_id_receiver.await.unwrap(), block.id());
}
