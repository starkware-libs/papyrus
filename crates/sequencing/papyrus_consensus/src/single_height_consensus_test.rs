use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use tokio;

use super::SingleHeightConsensus;
use crate::test_utils::{MockTestContext, TestBlock, TestMessages, TestNetworkSender};
use crate::types::{ConsensusBlock, ProposalInit, ValidatorId};

async fn setup(
    height: BlockNumber,
    id: ValidatorId,
    context: MockTestContext,
) -> (SingleHeightConsensus<TestBlock>, mpsc::Receiver<TestMessages>) {
    let (network_sender, network_receiver) = mpsc::channel(1);
    let test_network_sender = TestNetworkSender { sender: network_sender };
    let shc =
        SingleHeightConsensus::new(height, Arc::new(context), id, Box::new(test_network_sender))
            .await;
    (shc, network_receiver)
}

#[tokio::test]
async fn propose() {
    let mut context = MockTestContext::new();

    let node_id: ValidatorId = 1_u32.into();
    let block =
        TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::try_from(1 as u128).unwrap()) };
    // Set expectations for how the test should run:
    context
        .expect_validators()
        .returning(move |_| vec![node_id, 2_u32.into(), 3_u32.into(), 4_u32.into()]);
    context.expect_proposer().returning(move |_, _| node_id);
    let block_clone = block.clone();
    context.expect_build_proposal().returning(move |_| {
        let (mut content_sender, content_receiver) = mpsc::channel(1);
        let (block_sender, block_receiver) = oneshot::channel();

        let block = block_clone.clone();
        // While spawning makes the test more complex it gives us 2 benefits:
        // 1. We cannot use async code in the mock `returning` and sending on the channel is async.
        // 2. If we don't spawn (instead using `block_on` to send), then the channel size must be
        //    large enough for all content. Now we don't have to consider the size.
        let _ = tokio::spawn(async move {
            for chunk in block.proposal_iter() {
                content_sender.send(chunk).await.unwrap();
            }
            block_sender.send(block).unwrap();
        });

        (content_receiver, block_receiver)
    });

    // Creation calls to `context.validators`.
    let (mut shc, mut shc_to_network_receiver) = setup(BlockNumber(0), node_id, context).await;

    // This calls to `context.proposer` and `context.build_proposal`.
    let decision = shc.start().await.unwrap().unwrap();
    assert_eq!(decision, block);

    // Check what was sent to peering. We don't check the content stream as that is filled by
    // ConsensusContext, not SHC.
    let TestMessages::Proposal((init, proposed_block)) =
        shc_to_network_receiver.next().await.unwrap();
    assert_eq!(init, ProposalInit { height: BlockNumber(0), proposer: node_id });
    assert_eq!(
        proposed_block, decision,
        "TestNetworkSender built a different block from the proposal"
    );
}

#[tokio::test]
async fn validate() {
    let mut context = MockTestContext::new();

    let node_id: ValidatorId = 1_u32.into();
    let proposer: ValidatorId = 2_u32.into();
    let block =
        TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::try_from(1 as u128).unwrap()) };

    // Set expectations for how the test should run:
    context
        .expect_validators()
        .returning(move |_| vec![node_id, proposer, 3_u32.into(), 4_u32.into()]);
    context.expect_proposer().returning(move |_, _| proposer);
    let block_clone = block.clone();
    context.expect_validate_proposal().returning(move |_, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(block_clone.clone()).unwrap();
        block_receiver
    });

    // Creation calls to `context.validators`.
    let (mut shc, _) = setup(BlockNumber(0), node_id, context).await;

    // Send the proposal from the peer.
    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(block.id()).unwrap();
    let decision = shc
        .handle_proposal(
            ProposalInit { height: BlockNumber(0), proposer },
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )
        .await
        .unwrap()
        .unwrap();

    // This calls to `context.proposer` and `context.build_proposal`.
    assert_eq!(decision, block);
}
