#![warn(missing_docs)]
// TODO(Matan): Add a description of the crate.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [`Starknet`] node.
//!
//! This crate provides ...
//!
//! # Disclaimer
//! This crate is still under development and is not keeping backwards compatibility with previous
//! versions. Breaking changes are expected to happen in the near future.
//!
//! # Quick Start
//! ...

use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use papyrus_network::network_manager::SubscriberReceiver;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use single_height_consensus::SingleHeightConsensus;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::info;
use types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

// TODO(matan): Remove dead code allowance at the end of milestone 1.
#[allow(missing_docs)]
pub mod papyrus_consensus_context;
#[allow(dead_code)]
#[allow(missing_docs)]
pub mod single_height_consensus;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(dead_code)]
#[allow(missing_docs)]
pub mod types;

use futures::StreamExt;

// TODO(dvir): add test for this.
#[allow(missing_docs)]
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
        info!("Starting consensus for height {current_height}");
        let mut shc =
            SingleHeightConsensus::new(current_height, context.clone(), validator_id).await;

        let block = if let Some(block) = shc.start().await? {
            info!("Proposer flow height {current_height}");
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
            "Finished consensus for height: {start_height}. Agreed on block with id: {}",
            block.id()
        );
        current_height = current_height.unchecked_next();
    }
}

// `Proposal` is defined in the protobuf crate so we can't implement `Into` for it because of the
// orphan rule. This wrapper enables us to implement `Into` for the inner `Proposal`.
#[allow(missing_docs)]
pub struct ProposalWrapper(Proposal);
