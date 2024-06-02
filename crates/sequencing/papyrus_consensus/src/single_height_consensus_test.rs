use std::sync::{Arc, OnceLock};

use futures::channel::{mpsc, oneshot};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use tokio;

use super::SingleHeightConsensus;
use crate::test_utils::{MockTestContext, TestBlock};
use crate::types::{ConsensusBlock, MockNetworkSender, ProposalInit, ValidatorId};

#[tokio::test]
async fn proposer() {
    let mut context = MockTestContext::new();
    let mut network_sender = MockNetworkSender::new();

    let node_id: ValidatorId = 1_u32.into();
    let block = TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::ONE) };
    // Set expectations for how the test should run:
    context
        .expect_validators()
        .returning(move |_| vec![node_id, 2_u32.into(), 3_u32.into(), 4_u32.into()]);
    context.expect_proposer().returning(move |_, _| node_id);
    let block_clone = block.clone();
    context.expect_build_proposal().returning(move |_| {
        let (_, content_receiver) = mpsc::channel(1);
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(block_clone.clone()).unwrap();
        (content_receiver, block_receiver)
    });

    let fin_receiver = Arc::new(OnceLock::new());
    let fin_receiver_clone = Arc::clone(&fin_receiver);
    network_sender.expect_propose().return_once(move |init, _, fin_receiver| {
        // Ignore content receiver, since this is the context's responsibility.
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(init.proposer, node_id);
        fin_receiver_clone.set(fin_receiver).unwrap();
        Ok(())
    });

    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        Arc::new(context),
        node_id,
        Box::new(network_sender),
    )
    .await;

    let decision = shc.start().await.unwrap().unwrap();
    assert_eq!(decision, block);

    // Check the fin sent to the network.
    let fin = Arc::into_inner(fin_receiver).unwrap().take().unwrap().await.unwrap();
    assert_eq!(fin, block.id());
}

#[tokio::test]
async fn validator() {
    let mut context = MockTestContext::new();

    let node_id: ValidatorId = 1_u32.into();
    let proposer: ValidatorId = 2_u32.into();
    let block = TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::ONE) };

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
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        Arc::new(context),
        node_id,
        Box::new(MockNetworkSender::new()),
    )
    .await;

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
