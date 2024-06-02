use std::sync::{Arc, Mutex};

use futures::channel::{mpsc, oneshot};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use tokio;
use tokio::task::JoinSet;

use super::SingleHeightConsensus;
use crate::test_utils::{MockTestContext, TestBlock};
use crate::types::{ConsensusBlock, MockNetworkSender, ProposalInit, ValidatorId};

#[tokio::test]
async fn propose() {
    let mut context = MockTestContext::new();
    let mut network_sender = MockNetworkSender::new();
    let joinset = Arc::new(Mutex::new(JoinSet::new()));

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

        let block = block_clone.clone();
        block_sender.send(block).unwrap();

        (content_receiver, block_receiver)
    });

    let block_id = block.id();
    let joinset_clone = Arc::clone(&joinset);
    network_sender.expect_propose().returning(move |init, _, fin_receiver| {
        // Ignore content receiver, since this is the context's responsibility.
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(init.proposer, node_id);
        joinset_clone.lock().unwrap().spawn(async move {
            assert_eq!(fin_receiver.await.unwrap(), block_id);
        });
        Ok(())
    });

    // Creation calls to `context.validators`.
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        Arc::new(context),
        node_id,
        Box::new(network_sender),
    )
    .await;

    // This calls to `context.proposer` and `context.build_proposal`.
    let decision = shc.start().await.unwrap().unwrap();
    assert_eq!(decision, block);

    // Wait until all tasks complete.
    while let Some(_) = joinset.lock().unwrap().join_next().await {}
}

#[tokio::test]
async fn validate() {
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
