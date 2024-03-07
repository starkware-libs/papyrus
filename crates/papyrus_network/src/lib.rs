/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod bin_utils;
mod converters;
mod db_executor;
pub mod network_manager;
pub mod protobuf_messages;
pub mod streamed_bytes;
#[cfg(test)]
mod test_utils;
use std::collections::BTreeMap;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;
use std::usize;

#[cfg(test)]
use enum_iterator::Sequence;
use futures::Stream;
use libp2p::{PeerId, StreamProtocol};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_optional_sub_config, ser_param, SerializeConfig};
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
    pub peer: Option<PeerAddressConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PeerAddressConfig {
    pub peer_id: PeerId,
    pub ip: String,
    pub tcp_port: u16,
    // TODO: Add quic_port as optional, and make tcp_port optional as well while enforcing at least
    // one of them to have value
}

#[derive(Default, Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Sequence, Clone, Copy))]
pub enum DataType {
    #[default]
    SignedBlockHeader,
    StateDiff,
}

impl From<Protocol> for DataType {
    fn from(protocol: Protocol) -> DataType {
        match protocol {
            Protocol::SignedBlockHeader => DataType::SignedBlockHeader,
        }
    }
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
#[cfg_attr(test, derive(Clone))]
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
    pub signed_headers_receiver: Pin<Box<dyn Stream<Item = Option<SignedBlockHeader>> + Send>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Protocol {
    SignedBlockHeader,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::SignedBlockHeader => "/starknet/headers/1",
        }
    }
}

impl From<Protocol> for StreamProtocol {
    fn from(protocol: Protocol) -> StreamProtocol {
        StreamProtocol::new(protocol.as_str())
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Unknown protocol: {0}")]
pub struct UnknownProtocolConversionError(String);

impl TryFrom<StreamProtocol> for Protocol {
    type Error = UnknownProtocolConversionError;

    fn try_from(protocol: StreamProtocol) -> Result<Self, Self::Error> {
        match protocol.as_ref() {
            "/starknet/headers/1" => Ok(Protocol::SignedBlockHeader),
            _ => Err(UnknownProtocolConversionError(protocol.as_ref().to_string())),
        }
    }
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
        config.extend(ser_optional_sub_config(&self.peer, "peer"));
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
            peer: None,
        }
    }
}

impl SerializeConfig for PeerAddressConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip,
                "The ipv4 address of another peer that the node will dial to.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "tcp_port",
                &self.tcp_port,
                "The port on the other peer that the node will dial to to use for TCP transport.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "peer_id", 
                &self.peer_id,
                "Peer ID to send requests to. If not set, the node will not send requests. for info: https://docs.libp2p.io/concepts/fundamentals/peers/", ParamPrivacyInput::Public),
        ])
    }
}

// TODO: remove default implementation once config stops requiring it.
impl Default for PeerAddressConfig {
    fn default() -> Self {
        Self {
            peer_id: PeerId::from_str("QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N")
                .expect("QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N should be a valid peer ID"),
            ip: "127.0.0.1".to_string(),
            tcp_port: 10002,
        }
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
