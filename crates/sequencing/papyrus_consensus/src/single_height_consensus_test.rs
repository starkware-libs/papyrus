use std::sync::{Arc, OnceLock};

use futures::channel::{mpsc, oneshot};
use papyrus_protobuf::consensus::{ConsensusMessage, Vote, VoteType};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;
use tokio;

use super::SingleHeightConsensus;
use crate::test_utils::{MockTestContext, TestBlock};
use crate::types::{ConsensusBlock, ProposalInit, ValidatorId};

fn prevote(block_hash: BlockHash, height: u64, voter: ValidatorId) -> ConsensusMessage {
    ConsensusMessage::Vote(Vote { vote_type: VoteType::Prevote, height, block_hash, voter })
}

fn precommit(block_hash: BlockHash, height: u64, voter: ValidatorId) -> ConsensusMessage {
    ConsensusMessage::Vote(Vote { vote_type: VoteType::Precommit, height, block_hash, voter })
}

#[tokio::test]
async fn proposer() {
    let mut context = MockTestContext::new();

    let node_id: ValidatorId = 1_u32.into();
    let block = TestBlock { content: vec![1, 2, 3], id: BlockHash(Felt::ONE) };
    let block_id = block.id();
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
    context.expect_propose().return_once(move |init, _, fin_receiver| {
        // Ignore content receiver, since this is the context's responsibility.
        assert_eq!(init.height, BlockNumber(0));
        assert_eq!(init.proposer, node_id);
        fin_receiver_clone.set(fin_receiver).unwrap();
        Ok(())
    });
    context
        .expect_broadcast()
        .withf(move |msg: &ConsensusMessage| msg == &prevote(block_id, 0, node_id))
        .returning(move |_| Ok(()));
    context
        .expect_broadcast()
        .withf(move |msg: &ConsensusMessage| msg == &precommit(block_id, 0, node_id))
        .returning(move |_| Ok(()));

    let mut shc = SingleHeightConsensus::new(BlockNumber(0), Arc::new(context), node_id).await;

    // Sends proposal and prevote.
    assert!(matches!(shc.start().await, Ok(None)));

    assert_eq!(shc.handle_message(prevote(block.id(), 0, 2_u32.into())).await, Ok(None));
    assert_eq!(shc.handle_message(prevote(block.id(), 0, 3_u32.into())).await, Ok(None));
    assert_eq!(shc.handle_message(precommit(block.id(), 0, 2_u32.into())).await, Ok(None));
    let decision =
        shc.handle_message(precommit(block.id(), 0, 3_u32.into())).await.unwrap().unwrap();
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
    let block_id = block.id();

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
    context
        .expect_broadcast()
        .withf(move |msg: &ConsensusMessage| msg == &prevote(block_id, 0, node_id))
        .returning(move |_| Ok(()));
    context
        .expect_broadcast()
        .withf(move |msg: &ConsensusMessage| msg == &precommit(block_id, 0, node_id))
        .returning(move |_| Ok(()));

    // Creation calls to `context.validators`.
    let mut shc = SingleHeightConsensus::new(BlockNumber(0), Arc::new(context), node_id).await;

    // Send the proposal from the peer.
    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(block.id()).unwrap();

    let res = shc
        .handle_proposal(
            ProposalInit { height: BlockNumber(0), proposer },
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )
        .await;
    assert_eq!(res, Ok(None));

    assert_eq!(shc.handle_message(prevote(block.id(), 0, 2_u32.into())).await, Ok(None));
    assert_eq!(shc.handle_message(prevote(block.id(), 0, 3_u32.into())).await, Ok(None));
    assert_eq!(shc.handle_message(precommit(block.id(), 0, 2_u32.into())).await, Ok(None));

    let decision =
        shc.handle_message(precommit(block.id(), 0, 3_u32.into())).await.unwrap().unwrap();
    assert_eq!(decision, block);
}
