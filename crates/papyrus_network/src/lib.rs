/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod bin_utils;
pub mod block_headers;
mod db_executor;
pub mod messages;
pub mod network_manager;
pub mod streamed_data;
#[cfg(test)]
mod test_utils;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;
use std::usize;

use futures::channel::mpsc::{Receiver, Sender};
use libp2p::PeerId;
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct NetworkConfig {
    pub tcp_port: u16,
    pub quic_port: u16,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,
    pub header_buffer_size: usize,
    pub peer_id: Option<PeerId>,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub enum DataType {
    #[default]
    SignedBlockHeader,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Query {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: usize,
    pub step: usize,
    pub data_type: DataType,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[cfg_attr(test, derive(Hash))]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

#[derive(Debug)]
pub struct SignedBlockHeader {
    pub block_header: BlockHeader,
    pub signatures: Vec<BlockSignature>,
}

// TODO(shahak): Internalize this when we have a mixed behaviour.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(test, derive(Hash))]
pub struct InternalQuery {
    pub start_block: BlockHashOrNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(test, derive(Hash))]
pub enum BlockHashOrNumber {
    Hash(BlockHash),
    Number(BlockNumber),
}

pub struct ResponseReceivers {
    pub signed_headers_receiver: Receiver<SignedBlockHeader>,
}

struct ResponseSenders {
    pub signed_headers_sender: Sender<SignedBlockHeader>,
}

impl SerializeConfig for NetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "tcp_port",
                &self.tcp_port,
                "The port that the node listens on for incoming tcp connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "quic_port",
                &self.quic_port,
                "The port that the node listens on for incoming quic connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "session_timeout",
                &self.session_timeout.as_secs(),
                "Maximal time in seconds that each session can take before failing on timeout.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_connection_timeout",
                &self.idle_connection_timeout.as_secs(),
                "Amount of time in seconds that a connection with no active sessions will stay \
                 alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "header_buffer_size",
                &self.header_buffer_size,
                "Size of the buffer for headers read from the storage.",
                ParamPrivacyInput::Public,
            ),
        ]);
        let peer_id_example = PeerId::from_str("QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N")
            .expect("failed to parse peer id");
        config.extend(ser_optional_param(
                &self.peer_id,
                peer_id_example,
                "peer_id",
                "Peer ID to send requests to. If not set, the node will not send requests. for info: https://docs.libp2p.io/concepts/fundamentals/peers/",
                ParamPrivacyInput::Public,
            ));
        config
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            tcp_port: 10000,
            quic_port: 10001,
            session_timeout: Duration::from_secs(10),
            idle_connection_timeout: Duration::from_secs(10),
            header_buffer_size: 100000,
            peer_id: None,
        }
    }
}

impl ResponseReceivers {
    fn new(signed_headers_receiver: Receiver<SignedBlockHeader>) -> Self {
        Self { signed_headers_receiver }
    }
}

impl From<Query> for InternalQuery {
    fn from(query: Query) -> InternalQuery {
        InternalQuery {
            start_block: BlockHashOrNumber::Number(query.start_block),
            direction: query.direction,
            limit: query.limit as u64,
            step: query.step as u64,
        }
    }
}
