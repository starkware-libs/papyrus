#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::cell::RefCell;
use std::ops::DerefMut;
use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use starknet_api::block::BlockNumber;

use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
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
    // This is a RefCell since peering sends to the same receiver permanently, but a new SHC runs
    // for each height.
    from_peering_receiver: RefCell<mpsc::Receiver<PeeringConsensusMessage<BlockT::ProposalChunk>>>,
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
        from_peering_receiver: RefCell<
            mpsc::Receiver<PeeringConsensusMessage<BlockT::ProposalChunk>>,
        >,
    ) -> Self {
        let validators = context.validators(height).await;
        Self { height, context, validators, id, to_peering_sender, from_peering_receiver }
    }

    pub(crate) async fn run(mut self) -> Result<BlockT, ConsensusError> {
        // TODO(matan): In the future this logic will be encapsulated in the state machine, and SHC
        // will await a signal from SHC to propose.
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if proposer_id == self.id { self.propose().await } else { self.validate(proposer_id).await }
    }

    async fn propose(&mut self) -> Result<BlockT, ConsensusError> {
        let (content_receiver, block_receiver) = self.context.build_proposal(self.height).await;
        let (fin_sender, fin_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, proposer: self.id };
        // Peering is a permanent component, so if sending to it fails we cannot continue.
        self.to_peering_sender
            .send(PeeringConsensusMessage::Proposal((init, content_receiver, fin_receiver)))
            .await
            .expect("Failed sending Proposal to Peering");
        let block = block_receiver.await.expect("Block building failed.");
        // If we choose to ignore this error, we should carefully consider how this affects
        // Tendermint. The partially synchronous model assumes all messages arrive at some point,
        // and this failure means this proposal will never arrive.
        //
        // TODO(matan): Switch this to the Proposal signature.
        fin_sender.send(block.id()).expect("Failed to send ProposalFin to Peering.");
        Ok(block)
    }

    async fn validate(&mut self, proposer_id: NodeId) -> Result<BlockT, ConsensusError> {
        let mut receiver = self
            .from_peering_receiver
            .try_borrow_mut()
            .expect("Couldn't get exclusive access to Peering receiver.");
        // Peering is a permanent component, so if receiving from it fails we cannot continue.
        let msg = receiver.deref_mut().next().await.expect("Cannot receive from Peering");
        let (init, content_receiver, fin_receiver) = match msg {
            PeeringConsensusMessage::Proposal((init, content_receiver, block_hash_receiver)) => {
                (init, content_receiver, block_hash_receiver)
            }
        };
        if init.height != self.height || init.proposer != proposer_id {
            // Ignore the proposal.
            // TODO(matan): Do we want to handle this gracefully and "retry" in milestone 1?
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height));
        }
        let block_receiver = self.context.validate_proposal(self.height, content_receiver).await;
        // Receive the block which was build by Context. If the channel is closed without a block
        // being sent, this is an invalid proposal.
        // TODO(matan): Actual Tendermint should handle invalid proposals.
        let block = block_receiver
            .await
            .map_err(|_| ConsensusError::InvalidProposal(proposer_id, self.height))?;
        // Receive the fin message from the proposer. If this is not received, this is an invalid
        // proposal.
        // TODO(matan): Actual Tendermint should handle invalid proposals.
        let fin = fin_receiver
            .await
            .map_err(|_| ConsensusError::InvalidProposal(proposer_id, self.height))?;
        // TODO(matan): Switch to signature validation and handle invalid proposals.
        // An invalid signature means the proposal is invalid. The signature validation on the
        // proposal's init prevents a malicious node from sending proposals during another's round.
        if block.id() != fin {
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height));
        }
        Ok(block)
    }
}
