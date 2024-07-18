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

    let mut shc = SingleHeightConsensus::new(
        vec![node_id, 2_u32.into(), 3_u32.into(), 4_u32.into()],
        BlockNumber(0),
        node_id,
    );

    // Start will send the proposal and prevote.
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
    assert!(matches!(shc.start(&context).await, Ok(None)));

    assert_eq!(shc.handle_message(&context, prevote(block.id(), 0, 2_u32.into())).await, Ok(None));
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .withf(move |msg: &ConsensusMessage| msg == &precommit(block_id, 0, node_id))
        .returning(move |_| Ok(()));
    assert_eq!(shc.handle_message(&context, prevote(block.id(), 0, 3_u32.into())).await, Ok(None));

    let precommits = vec![
        precommit(block.id(), 0, 1_u32.into()),
        precommit(BlockHash(Felt::TWO), 0, 4_u32.into()), // Ignores since disagrees.
        precommit(block.id(), 0, 2_u32.into()),
        precommit(block.id(), 0, 3_u32.into()),
    ];
    assert_eq!(shc.handle_message(&context, precommits[1].clone()).await, Ok(None));
    assert_eq!(shc.handle_message(&context, precommits[2].clone()).await, Ok(None));
    let decision = shc.handle_message(&context, precommits[3].clone()).await.unwrap().unwrap();
    assert_eq!(decision.block, block);
    assert!(
        decision
            .precommits
            .into_iter()
            .all(|item| precommits.contains(&ConsensusMessage::Vote(item)))
    );

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

    let mut shc = SingleHeightConsensus::new(
        vec![node_id, proposer, 3_u32.into(), 4_u32.into()],
        BlockNumber(0),
        node_id,
    );

    // Send the proposal from the peer.
    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(block.id()).unwrap();

    // Give the proposal to SHC.
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
    let res = shc
        .handle_proposal(
            &context,
            ProposalInit { height: BlockNumber(0), proposer },
            mpsc::channel(1).1, // content - ignored by SHC.
            fin_receiver,
        )
        .await;
    assert_eq!(res, Ok(None));

    assert_eq!(shc.handle_message(&context, prevote(block.id(), 0, 2_u32.into())).await, Ok(None));
    // 3 of 4 Prevotes is enough to send a Precommit.
    context
        .expect_broadcast()
        .withf(move |msg: &ConsensusMessage| msg == &precommit(block_id, 0, node_id))
        .returning(move |_| Ok(()));
    assert_eq!(shc.handle_message(&context, prevote(block.id(), 0, 3_u32.into())).await, Ok(None));

    let precommits = vec![
        precommit(block.id(), 0, 2_u32.into()),
        precommit(block.id(), 0, 3_u32.into()),
        precommit(block.id(), 0, node_id),
    ];
    assert_eq!(shc.handle_message(&context, precommits[0].clone()).await, Ok(None));
    let decision = shc.handle_message(&context, precommits[1].clone()).await.unwrap().unwrap();
    assert_eq!(decision.block, block);
    assert!(
        decision
            .precommits
            .into_iter()
            .all(|item| precommits.contains(&ConsensusMessage::Vote(item)))
    );
}
