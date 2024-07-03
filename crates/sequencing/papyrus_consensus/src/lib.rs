#![warn(missing_docs)]
// TODO(Matan): Add a description of the crate.
// TODO(Matan): fix #[allow(missing_docs)].
//! A consensus implementation for a [`Starknet`](https://www.starknet.io/) node.

use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use papyrus_network::network_manager::SubscriberReceiver;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use single_height_consensus::SingleHeightConsensus;
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};
use types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};

// TODO(matan): Remove dead code allowance at the end of milestone 1.
#[allow(missing_docs)]
pub mod papyrus_consensus_context;
#[allow(dead_code)]
#[allow(missing_docs)]
pub mod single_height_consensus;
#[allow(missing_docs)]
pub mod state_machine;
#[cfg(test)]
pub(crate) mod test_utils;
#[allow(dead_code)]
#[allow(missing_docs)]
pub mod types;

use std::collections::BTreeMap;

use futures::StreamExt;
use papyrus_config::dumping::{ser_param, ser_required_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};

/// Configuration for consensus.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ConsensusConfig {
    /// The validator ID of the node.
    pub validator_id: ValidatorId,
    /// The network topic of the consensus.
    pub topic: String,
    /// The height to start the consensus from.
    pub start_height: BlockNumber,
}
impl SerializeConfig for ConsensusConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_required_param(
                "validator_id",
                SerializationType::String,
                "The validator id of the node.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "topic",
                &self.topic,
                "The topic of the consensus.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "start_height",
                &self.start_height,
                "The height to start the consensus from.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            validator_id: ValidatorId::default(),
            topic: "consensus".to_string(),
            start_height: BlockNumber::default(),
        }
    }
}

// TODO(dvir): add test for this.
#[instrument(skip(context, start_height, network_receiver), level = "info")]
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
                .expect("Network receiver closed unexpectedly")
            else {
                todo!("Handle votes");
            };
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
#[allow(missing_docs)]
pub struct ProposalWrapper(Proposal);
