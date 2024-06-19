use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use papyrus_network::network_manager::SubscriberReceiver;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use single_height_consensus::SingleHeightConsensus;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};
use types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

// TODO(matan): Remove dead code allowance at the end of milestone 1.
pub mod papyrus_consensus_context;
#[allow(dead_code)]
pub mod single_height_consensus;
pub mod state_machine;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(dead_code)]
pub mod types;

use futures::StreamExt;

// TODO(dvir): add test for this.
#[instrument(
    skip(context, start_height, network_receiver),
    fields(validator_id = %validator_id)
)]
pub async fn run_consensus<BlockT: ConsensusBlock>(
    context: Arc<dyn ConsensusContext<Block = BlockT>>,
    start_height: BlockNumber,
    validator_id: ValidatorId,
    mut network_receiver: SubscriberReceiver<ConsensusMessage>,
) -> Result<(), ConsensusError>
where
    ProposalWrapper:
        Into<(ProposalInit, mpsc::Receiver<BlockT::ProposalChunk>, oneshot::Receiver<BlockHash>)>,
{
    let mut current_height = start_height;
    loop {
        debug!("Starting consensus for height {current_height}");
        let mut shc =
            SingleHeightConsensus::new(current_height, context.clone(), validator_id).await;

        let block = if let Some(block) = shc.start().await? {
            block
        } else {
            info!("Validator flow height {current_height}");
            let ConsensusMessage::Proposal(proposal) = network_receiver
                .next()
                .await
                .expect("Failed to receive a message from network")
                .0
                .expect("Network receiver closed unexpectedly");
            let (proposal_init, content_receiver, fin_receiver) = ProposalWrapper(proposal).into();

            shc.handle_proposal(proposal_init, content_receiver, fin_receiver)
                .await?
                .expect("Failed to handle proposal")
        };

        info!(
            "Finished consensus for height: {current_height}. Agreed on block with id: {:x}",
            block.id().0
        );
        current_height = current_height.unchecked_next();
    }
}

// `Proposal` is defined in the protobuf crate so we can't implement `Into` for it because of the
// orphan rule. This wrapper enables us to implement `Into` for the inner `Proposal`.
pub struct ProposalWrapper(Proposal);
