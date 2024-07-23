#![warn(missing_docs)]
// TODO(Matan): Add a description of the crate.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [`Starknet`](https://www.starknet.io/) node.

use std::time::Duration;

use futures::channel::{mpsc, oneshot};
use papyrus_common::metrics as papyrus_metrics;
use papyrus_network::network_manager::BroadcastSubscriberReceiver;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use single_height_consensus::SingleHeightConsensus;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};
use types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    Decision,
    ProposalInit,
    ValidatorId,
};

pub mod config;
#[allow(missing_docs)]
pub mod papyrus_consensus_context;
#[allow(missing_docs)]
pub mod single_height_consensus;
#[allow(missing_docs)]
pub mod state_machine;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(missing_docs)]
pub mod types;

use futures::StreamExt;

#[instrument(skip(context, validator_id, network_receiver, cached_messages), level = "info")]
#[allow(missing_docs)]
async fn run_height<BlockT: ConsensusBlock, ContextT: ConsensusContext<Block = BlockT>>(
    context: &mut ContextT,
    height: BlockNumber,
    validator_id: ValidatorId,
    network_receiver: &mut BroadcastSubscriberReceiver<ConsensusMessage>,
    cached_messages: &mut Vec<ConsensusMessage>,
) -> Result<Decision<BlockT>, ConsensusError>
where
    ProposalWrapper:
        Into<(ProposalInit, mpsc::Receiver<BlockT::ProposalChunk>, oneshot::Receiver<BlockHash>)>,
{
    let validators = context.validators(height).await;
    let mut shc = SingleHeightConsensus::new(height, validator_id, validators);

    if let Some(decision) = shc.start(context).await? {
        return Ok(decision);
    }

    let mut current_height_messages = Vec::new();
    for msg in std::mem::take(cached_messages) {
        match height.0.cmp(&msg.height()) {
            std::cmp::Ordering::Less => cached_messages.push(msg),
            std::cmp::Ordering::Equal => current_height_messages.push(msg),
            std::cmp::Ordering::Greater => {}
        }
    }

    loop {
        let message = if let Some(msg) = current_height_messages.pop() {
            msg
        } else {
            // TODO(matan): Handle parsing failures and utilize ReportCallback.
            network_receiver
                .next()
                .await
                .expect("Network receiver closed unexpectedly")
                .0
                .expect("Failed to parse consensus message")
        };

        if message.height() != height.0 {
            debug!("Received a message for a different height. {:?}", message);
            if message.height() > height.0 {
                cached_messages.push(message);
            }
            continue;
        }

        let maybe_decision = match message {
            ConsensusMessage::Proposal(proposal) => {
                // Special case due to fake streaming.
                let (proposal_init, content_receiver, fin_receiver) =
                    ProposalWrapper(proposal).into();
                shc.handle_proposal(context, proposal_init, content_receiver, fin_receiver).await?
            }
            _ => shc.handle_message(context, message).await?,
        };

        if let Some(decision) = maybe_decision {
            return Ok(decision);
        }
    }
}

// TODO(dvir): add test for this.
#[instrument(skip(context, start_height, network_receiver), level = "info")]
#[allow(missing_docs)]
pub async fn run_consensus<BlockT: ConsensusBlock, ContextT: ConsensusContext<Block = BlockT>>(
    mut context: ContextT,
    start_height: BlockNumber,
    validator_id: ValidatorId,
    consensus_delay: Duration,
    mut network_receiver: BroadcastSubscriberReceiver<ConsensusMessage>,
) -> Result<(), ConsensusError>
where
    ProposalWrapper:
        Into<(ProposalInit, mpsc::Receiver<BlockT::ProposalChunk>, oneshot::Receiver<BlockHash>)>,
{
    // Add a short delay to allow peers to connect and avoid "InsufficientPeers" error
    tokio::time::sleep(consensus_delay).await;
    let mut current_height = start_height;
    let mut future_messages = Vec::new();
    loop {
        let decision = run_height(
            &mut context,
            current_height,
            validator_id,
            &mut network_receiver,
            &mut future_messages,
        )
        .await?;

        info!(
            "Finished consensus for height: {current_height}. Agreed on block with id: {:x}",
            decision.block.id().0
        );
        debug!("Decision: {:?}", decision);
        metrics::gauge!(papyrus_metrics::PAPYRUS_CONSENSUS_HEIGHT, current_height.0 as f64);
        current_height = current_height.unchecked_next();
    }
}

// `Proposal` is defined in the protobuf crate so we can't implement `Into` for it because of the
// orphan rule. This wrapper enables us to implement `Into` for the inner `Proposal`.
#[allow(missing_docs)]
pub struct ProposalWrapper(Proposal);
