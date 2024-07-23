//! This module contains the configuration for consensus, including the `ConsensusConfig` struct
//! and its implementation of the `SerializeConfig` trait. The configuration includes parameters
//! such as the validator ID, the network topic of the consensus, and the starting block height.

use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, ser_required_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;

use super::types::ValidatorId;

/// Configuration for consensus.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ConsensusConfig {
    /// The validator ID of the node.
    pub validator_id: ValidatorId,
    /// The network topic of the consensus.
    pub topic: String,
    /// The height to start the consensus from.
    pub start_height: BlockNumber,
    /// The number of validators in the consensus.
    // Used for testing in an early milestones.
    pub num_validators: u64,
    /// The delay in seconds before starting consensus to prevent InsufficientPeers error.
    pub consensus_delay: u64,
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
            ser_param(
                "num_validators",
                &self.num_validators,
                "The number of validators in the consensus.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "consensus_delay",
                &self.consensus_delay,
                "The delay in seconds before starting consensus to prevent 'InsufficientPeers' \
                 error.",
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
            num_validators: 4,
            consensus_delay: 5,
        }
    }
}
