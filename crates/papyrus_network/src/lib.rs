/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod bin_utils;
mod broadcast;
mod converters;
mod db_executor;
mod discovery;
pub mod mixed_behaviour;
pub mod network_manager;
mod peer_manager;
pub mod protobuf_messages;
pub mod streamed_bytes;
#[cfg(test)]
mod test_utils;
mod utils;

use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::time::Duration;
use std::usize;

use bytes::BufMut;
use derive_more::Display;
use enum_iterator::Sequence;
use futures::Stream;
use lazy_static::lazy_static;
use libp2p::{Multiaddr, StreamProtocol};
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use prost::{EncodeError, Message};
use protobuf_messages::protobuf;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::state::ThinStateDiff;

// TODO: add peer manager config to the network config
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct NetworkConfig {
    pub tcp_port: u16,
    pub quic_port: u16,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,
    pub header_buffer_size: usize,
    pub bootstrap_peer_multiaddr: Option<Multiaddr>,
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Display)]
#[cfg_attr(test, derive(Sequence))]
pub enum DataType {
    // TODO: consider adding a default variant / removing the #[default] attribute.
    #[default]
    #[display(fmt = "SignedBlockHeader")]
    SignedBlockHeader,
    #[display(fmt = "StateDiff")]
    StateDiff,
}

impl From<Protocol> for DataType {
    fn from(protocol: Protocol) -> DataType {
        match protocol {
            Protocol::SignedBlockHeader => DataType::SignedBlockHeader,
            Protocol::StateDiff => DataType::StateDiff,
        }
    }
}

impl From<DataType> for Protocol {
    fn from(data_type: DataType) -> Protocol {
        match data_type {
            DataType::SignedBlockHeader => Protocol::SignedBlockHeader,
            DataType::StateDiff => Protocol::StateDiff,
        }
    }
}

/// This struct represents a query that can be sent to a peer.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Query {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: usize,
    pub step: usize,
    pub data_type: DataType,
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to encode query")]
pub struct QueryEncodingError;

impl Query {
    pub fn encode<B>(self, buf: &mut B) -> Result<(), QueryEncodingError>
    where
        B: BufMut,
    {
        match self.data_type {
            DataType::SignedBlockHeader => {
                <Query as Into<protobuf::BlockHeadersRequest>>::into(self).encode(buf)
            }
            DataType::StateDiff => {
                <Query as Into<protobuf::StateDiffsRequest>>::into(self).encode(buf)
            }
        }
        .map_err(|_: EncodeError| QueryEncodingError)
    }
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

pub type SignedBlockHeaderStream = Pin<Box<dyn Stream<Item = Option<SignedBlockHeader>> + Send>>;
pub type StateDiffStream = Pin<Box<dyn Stream<Item = Option<ThinStateDiff>> + Send>>;

/// This struct represents the receiver end of the response streams for a network subscriber.
/// It is created by the network manager and passed to the subscriber when calling
/// [`register_sqmr_subscriber`](`network_manager::GenericNetworkManager::register_sqmr_subscriber`).
pub struct ResponseReceivers {
    pub signed_headers_receiver: Option<SignedBlockHeaderStream>,
    pub state_diffs_receiver: Option<StateDiffStream>,
}

/// This is a part of the exposed API of the network manager.
/// This is meant to represent the different underlying p2p protocols the network manager supports.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Sequence)]
pub enum Protocol {
    SignedBlockHeader,
    StateDiff,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::SignedBlockHeader => "/starknet/headers/1",
            Protocol::StateDiff => "/starknet/state_diffs/1",
        }
    }

    pub fn bytes_query_to_protobuf_request(&self, query: Vec<u8>) -> InternalQuery {
        // TODO: make this function return errors instead of panicking.
        match self {
            Protocol::SignedBlockHeader => protobuf::BlockHeadersRequest::decode(&query[..])
                .expect("failed to decode protobuf BlockHeadersRequest")
                .try_into()
                .expect("failed to convert BlockHeadersRequest"),
            Protocol::StateDiff => protobuf::StateDiffsRequest::decode(&query[..])
                .expect("failed to decode protobuf StateDiffsRequest")
                .try_into()
                .expect("failed to convert StateDiffsRequest"),
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

lazy_static! {
    static ref PROTOCOL_NAME_TO_PROTOCOL: HashMap<&'static str, Protocol> =
        enum_iterator::all::<Protocol>().map(|protocol| (protocol.as_str(), protocol)).collect();
}

impl TryFrom<StreamProtocol> for Protocol {
    type Error = UnknownProtocolConversionError;

    fn try_from(protocol: StreamProtocol) -> Result<Self, Self::Error> {
        PROTOCOL_NAME_TO_PROTOCOL
            .get(protocol.as_ref())
            .ok_or(UnknownProtocolConversionError(protocol.as_ref().to_string()))
            .copied()
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
        config.extend(ser_optional_param(
            &self.bootstrap_peer_multiaddr,
            Multiaddr::empty(),
            "bootstrap_peer_multiaddr",
            "The multiaddress of the peer node. It should include the peer's id. For more info: https://docs.libp2p.io/concepts/fundamentals/peers/",
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
            session_timeout: Duration::from_secs(120),
            idle_connection_timeout: Duration::from_secs(120),
            header_buffer_size: 100000,
            bootstrap_peer_multiaddr: None,
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
