#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};

use crate::types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

/// Struct which represents a single height of consensus. Each height is expected to be begun with a
/// call to `start`, which is relevant if we are the proposer for this height's first round. SHC
/// receives messages directly as parameters to function calls. It can send out messages "directly"
/// to the network, and returning a decision to the caller.
pub(crate) struct SingleHeightConsensus<BlockT>
where
    BlockT: ConsensusBlock,
{
    height: BlockNumber,
    context: Arc<dyn ConsensusContext<Block = BlockT>>,
    validators: Vec<ValidatorId>,
    id: ValidatorId,
}

impl<BlockT> SingleHeightConsensus<BlockT>
where
    BlockT: ConsensusBlock,
{
    pub(crate) async fn new(
        height: BlockNumber,
        context: Arc<dyn ConsensusContext<Block = BlockT>>,
        id: ValidatorId,
    ) -> Self {
        let validators = context.validators(height).await;
        Self { height, context, validators, id }
    }

    #[instrument(skip(self), fields(height=self.height.0), level = "debug")]
    pub(crate) async fn start(&mut self) -> Result<Option<BlockT>, ConsensusError> {
        info!("Starting consensus with validators {:?}", self.validators);

        let proposer_id = self.context.proposer(&self.validators, self.height);
        if proposer_id != self.id {
            return Ok(None);
        }
        debug!("Proposer flow");

        let (content_receiver, block_receiver) = self.context.build_proposal(self.height).await;
        let (fin_sender, fin_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, proposer: self.id };
        // Peering is a permanent component, so if sending to it fails we cannot continue.
        self.context
            .propose(init, content_receiver, fin_receiver)
            .await
            .expect("Failed sending Proposal to Peering");
        let block = block_receiver.await.expect("Block building failed.");
        // If we choose to ignore this error, we should carefully consider how this affects
        // Tendermint. The partially synchronous model assumes all messages arrive at some point,
        // and this failure means this proposal will never arrive.
        //
        // TODO(matan): Switch this to the Proposal signature.
        fin_sender.send(block.id()).expect("Failed to send ProposalFin to Peering.");
        Ok(Some(block))
    }

    /// Receive a proposal from a peer node. Returns only once the proposal has been fully received
    /// and processed.
    #[instrument(
        skip(self, init, content_receiver, fin_receiver),
        fields(height = %self.height),
        level = "debug",
    )]
    pub(crate) async fn handle_proposal(
        &mut self,
        init: ProposalInit,
        content_receiver: mpsc::Receiver<<BlockT as ConsensusBlock>::ProposalChunk>,
        fin_receiver: oneshot::Receiver<BlockHash>,
    ) -> Result<Option<BlockT>, ConsensusError> {
        debug!(
            "Received proposal: proposal_height={}, proposer={:?}",
            init.height.0, init.proposer
        );
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if init.height != self.height {
            let msg = format!("invalid height: expected {:?}, got {:?}", self.height, init.height);
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height, msg));
        }
        if init.proposer != proposer_id {
            let msg =
                format!("invalid proposer: expected {:?}, got {:?}", proposer_id, init.proposer);
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height, msg));
        }

        let block_receiver = self.context.validate_proposal(self.height, content_receiver).await;
        // TODO(matan): Actual Tendermint should handle invalid proposals.
        let block = block_receiver.await.map_err(|_| {
            ConsensusError::InvalidProposal(
                proposer_id,
                self.height,
                "block validation failed".into(),
            )
        })?;
        // TODO(matan): Actual Tendermint should handle invalid proposals.
        let fin = fin_receiver.await.map_err(|_| {
            ConsensusError::InvalidProposal(
                proposer_id,
                self.height,
                "proposal fin never received".into(),
            )
        })?;
        // TODO(matan): Switch to signature validation and handle invalid proposals.
        if block.id() != fin {
            return Err(ConsensusError::InvalidProposal(
                proposer_id,
                self.height,
                "block signature doesn't match expected block hash".into(),
            ));
        }
        Ok(Some(block))
    }
}
