#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use starknet_api::block::BlockNumber;

use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    NodeId,
    PeeringConsensusMessage,
    ProposalInit,
};

pub(crate) struct SingleHeightConsensus<BlockT>
where
    BlockT: ConsensusBlock,
{
    height: BlockNumber,
    context: Arc<dyn ConsensusContext<Block = BlockT>>,
    validators: Vec<NodeId>,
    id: NodeId,
    to_peering_sender: mpsc::Sender<PeeringConsensusMessage<BlockT::ProposalChunk>>,
    from_peering_receiver: mpsc::Receiver<PeeringConsensusMessage<BlockT::ProposalChunk>>,
}

impl<BlockT> SingleHeightConsensus<BlockT>
where
    BlockT: ConsensusBlock,
{
    pub(crate) async fn new(
        height: BlockNumber,
        context: Arc<dyn ConsensusContext<Block = BlockT>>,
        id: NodeId,
        to_peering_sender: mpsc::Sender<PeeringConsensusMessage<BlockT::ProposalChunk>>,
        from_peering_receiver: mpsc::Receiver<PeeringConsensusMessage<BlockT::ProposalChunk>>,
    ) -> Self {
        let validators = context.validators(height).await;
        Self { height, context, validators, id, to_peering_sender, from_peering_receiver }
    }

    pub(crate) async fn run(mut self) -> BlockT {
        // TODO: In the future this logic will be encapsulated in the state machine, and SHC will
        // await a signal from SHC to propose.
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if proposer_id == self.id {
            self.propose().await
        } else {
            todo!("run as validator");
        }
    }

    async fn propose(&mut self) -> BlockT {
        let (content_receiver, block_receiver) = self.context.build_proposal(self.height).await;
        let (block_hash_sender, block_hash_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, proposer: self.id };
        self.to_peering_sender
            .send(PeeringConsensusMessage::Proposal((init, content_receiver, block_hash_receiver)))
            .await
            .expect("failed to send proposal to peering");
        let block = block_receiver.await.expect("failed to build block");
        block_hash_sender.send(block.id()).expect("failed to send block hash");
        block
    }
}
