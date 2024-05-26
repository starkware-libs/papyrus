#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
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
        // TODO(matan): In the future this logic will be encapsulated in the state machine, and SHC
        // will await a signal from SHC to propose.
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if proposer_id == self.id { self.propose().await } else { self.validate(proposer_id).await }
    }

    async fn propose(&mut self) -> BlockT {
        let (content_receiver, block_receiver) = self.context.build_proposal(self.height).await;
        let (fin_sender, fin_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, proposer: self.id };
        self.to_peering_sender
            .send(PeeringConsensusMessage::Proposal((init, content_receiver, fin_receiver)))
            .await
            .expect("failed to send proposal to peering");
        let block = block_receiver.await.expect("failed to build block");
        // TODO: Switch this to the Proposal signature.
        fin_sender.send(block.id()).expect("failed to send block hash");
        block
    }

    async fn validate(&mut self, proposer_id: NodeId) -> BlockT {
        let Some(msg) = self.from_peering_receiver.next().await else {
            panic!("Peering component disconnected from SingleHeightConsensus");
        };

        let (init, content_receiver, fin_receiver) = match msg {
            PeeringConsensusMessage::Proposal((init, content_receiver, block_hash_receiver)) => {
                (init, content_receiver, block_hash_receiver)
            }
        };
        assert_eq!(init.height, self.height);
        assert_eq!(init.proposer, proposer_id);
        let block_receiver = self.context.validate_proposal(self.height, content_receiver).await;
        let block = block_receiver.await.expect("failed to build block");
        let fin = fin_receiver.await.expect("failed to receive block hash");
        // TODO Switch to signature validation.
        assert_eq!(block.id(), fin);
        block
    }
}
