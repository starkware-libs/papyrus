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
