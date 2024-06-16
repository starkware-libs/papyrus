use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use papyrus_protobuf::consensus::ConsensusMessage;
use single_height_consensus::SingleHeightConsensus;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::info;
use types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

// TODO(matan): Remove dead code allowance at the end of milestone 1.
pub mod papyrus_consensus_context;
#[allow(dead_code)]
pub mod single_height_consensus;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(dead_code)]
pub mod types;

// TODO(dvir): add test for this.
pub async fn run_consensus<BlockT: ConsensusBlock>(
    context: Arc<dyn ConsensusContext<Block = BlockT>>,
    start_height: BlockNumber,
    validator_id: ValidatorId,
    mut network_receiver: mpsc::Receiver<ConsensusMessage>,
) -> Result<(), ConsensusError>
where
    papyrus_protobuf::consensus::Proposal:
        Into<(ProposalInit, mpsc::Receiver<BlockT::ProposalChunk>, oneshot::Receiver<BlockHash>)>,
{
    let mut current_height = start_height;
    loop {
        info!("Starting consensus for height {start_height}");
        let mut shc =
            SingleHeightConsensus::new(current_height, context.clone(), validator_id).await;

        let block = if let Some(block) = shc.start().await? {
            info!("Proposer flow height {current_height}");
            block
        } else {
            info!("Validator flow height {current_height}");
            let ConsensusMessage::Proposal(proposal) = network_receiver
                .try_next()
                .expect("Failed to receive a message from network")
                .expect("Network receiver closed unexpectedly");
            let (proposal_init, content_receiver, fin_receiver) = proposal.into();

            shc.handle_proposal(proposal_init, content_receiver, fin_receiver).await?.unwrap()
        };

        info!(
            "Finished consensus for height: {start_height}. Agreed on block with id: {}",
            block.id()
        );
        current_height = current_height.unchecked_next();
    }
}
