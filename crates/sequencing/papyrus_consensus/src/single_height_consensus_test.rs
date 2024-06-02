use std::cell::RefCell;
use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use starknet_api::block::BlockNumber;
use tokio;

use super::SingleHeightConsensus;
use crate::test_utils::{MockTestContext, TestBlock};
use crate::types::{ConsensusBlock, NodeId, PeeringConsensusMessage, ProposalInit};

type Shc = SingleHeightConsensus<TestBlock>;
type ProposalChunk = <TestBlock as ConsensusBlock>::ProposalChunk;
type PeeringMessage = PeeringConsensusMessage<ProposalChunk>;

struct TestSetup {
    pub context: MockTestContext,
    pub shc_to_peering_sender: mpsc::Sender<PeeringConsensusMessage<u32>>,
    pub shc_to_peering_receiver: mpsc::Receiver<PeeringConsensusMessage<u32>>,
    pub peering_to_shc_sender: mpsc::Sender<PeeringConsensusMessage<u32>>,
    pub peering_to_shc_receiver: mpsc::Receiver<PeeringConsensusMessage<u32>>,
}

impl TestSetup {
    fn new() -> TestSetup {
        let (shc_to_peering_sender, shc_to_peering_receiver) = mpsc::channel(1);
        let (peering_to_shc_sender, peering_to_shc_receiver) = mpsc::channel(1);
        let context = MockTestContext::new();
        TestSetup {
            context,
            shc_to_peering_sender,
            shc_to_peering_receiver,
            peering_to_shc_sender,
            peering_to_shc_receiver,
        }
    }

    // This should be called after all of the mock's expectations have been set. This is because SHC
    // holds `Arc<dyn ConsentContext>`. Setting mock expectations, though, require exclusive access
    // (`&mut self`).
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
            RefCell::new(self.peering_to_shc_receiver),
        )
        .await;
        (shc, self.shc_to_peering_receiver, self.peering_to_shc_sender)
    }
}

#[tokio::test]
async fn propose() {
    let mut test_fields = TestSetup::new();
    let node_id: NodeId = 1_u32.into();
    let block = TestBlock { content: vec![1, 2, 3], id: 1 };
    // Set expectations for how the test should run:
    test_fields
        .context
        .expect_validators()
        .returning(move |_| vec![node_id, 2_u32.into(), 3_u32.into(), 4_u32.into()]);
    test_fields.context.expect_proposer().returning(move |_, _| node_id);
    let block_clone = block.clone();
    test_fields.context.expect_build_proposal().returning(move |_| {
        // SHC doesn't actually handle the content, so ignore for unit tests.
        let (_, content_receiver) = mpsc::channel(1);
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(block_clone.clone()).unwrap();
        (content_receiver, block_receiver)
    });

    // Creation calls to `context.validators`.
    let (shc, mut shc_to_peering_receiver, _) = test_fields.new_shc(BlockNumber(0), node_id).await;

    // This calls to `context.proposer` and `context.build_proposal`.
    assert_eq!(shc.run().await.unwrap(), block);

    // Check what was sent to peering. We don't check the content stream as that is filled by
    // ConsensusContext, not SHC.
    let PeeringMessage::Proposal((init, _, block_id_receiver)) =
        shc_to_peering_receiver.next().await.unwrap();
    assert_eq!(init, ProposalInit { height: BlockNumber(0), proposer: node_id });
    assert_eq!(block_id_receiver.await.unwrap(), block.id());
}

#[tokio::test]
async fn validate() {
    let mut test_fields = TestSetup::new();
    let node_id: NodeId = 1_u32.into();
    let proposer: NodeId = 2_u32.into();
    let block = TestBlock { content: vec![1, 2, 3], id: 1 };
    // Set expectations for how the test should run:
    test_fields
        .context
        .expect_validators()
        .returning(move |_| vec![node_id, proposer, 3_u32.into(), 4_u32.into()]);
    test_fields.context.expect_proposer().returning(move |_, _| proposer);
    let block_clone = block.clone();
    test_fields.context.expect_validate_proposal().returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(block_clone.clone()).unwrap();
        block_receiver
    });

    // Creation calls to `context.validators`.
    let (shc, _, mut peering_to_shc_sender) = test_fields.new_shc(BlockNumber(0), node_id).await;

    // Send the proposal from the peer.
    let (fin_sender, fin_receiver) = oneshot::channel();
    peering_to_shc_sender
        .send(PeeringMessage::Proposal((
            ProposalInit { height: BlockNumber(0), proposer },
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )))
        .await
        .unwrap();
    fin_sender.send(block.id()).unwrap();

    // This calls to `context.proposer` and `context.build_proposal`.
    assert_eq!(shc.run().await.unwrap(), block);
}
