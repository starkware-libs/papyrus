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
        // This means that Context failed when building a proposal. While Tendermint is able to
        // handle invalid proposals, I think it's more straightforward to panic here. If needed, we
        // can change this to work like an invalid proposal.
        let block = block_receiver.await.expect("Block building failed.");
        // This means that we cannot finish sending the proposal to peers. Specifically this
        // indicates a bug in Peering that causes it to drop the receiver.
        //
        // In theory, it may be possible to simply ignore this failure and keep running consensus.
        // This is because Tendermint already must handle network failures. Other nodes would
        // timeout and run LOC 57. There is a subtle caveat though: the partially synchronous model
        // assumes all messages will arrive post-GST. A failure here means that this Proposal will
        // never arrive. We therefore should be careful in our consideration before ignoring this
        // issue.
        // TODO(matan): Switch this to the Proposal signature.
        fin_sender.send(block.id()).expect("Failed to send ProposalFin to Peering.");
        Ok(block)
    }

    async fn validate(&mut self, proposer_id: NodeId) -> Result<BlockT, ConsensusError> {
        // Peering is a permanent component, so if receiving from it fails we cannot continue.
        let msg = self.from_peering_receiver.next().await.expect("Cannot receive from Peering");
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
        // TODO(matan): Switch to signature validation.
        // In the future the fin will be a signature on the entire proposal, which includes the
        // block hash calculated from building the proposal. Since we calculate the block hash from
        // the content, an invalid signature will be presumed to be caused by the proposer
        // calculating a different block hash calculated by the sender. This will be treated as an
        // invalid proposal.
        //
        // It is safe to treat this as an invalid proposal, instead of ignoring, because the init
        // message of the stream must be signed by the correct proposer or else we simply ignore it.
        // This prevents malicious nodes from sending invalid proposals when it is not their turn
        // as proposer.
        if block.id() != fin {
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height));
        }
        Ok(block)
    }
}
