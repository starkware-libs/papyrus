pub mod client;
pub mod server;

use std::collections::HashMap;

use derive_more::Display;
use enum_iterator::Sequence;
use lazy_static::lazy_static;

pub const BUFFER_SIZE: usize = 100000;
/// This is a part of the exposed API of the network manager.
/// This is meant to represent the different underlying p2p protocols the network manager supports.
// TODO(shahak): Change protocol to a wrapper of string.
#[derive(Debug, Display, PartialEq, Eq, Clone, Copy, Hash, Sequence)]
pub enum Protocol {
    SignedBlockHeader,
    StateDiff,
    Transaction,
    Class,
    Event,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::SignedBlockHeader => "/starknet/headers/1",
            Protocol::StateDiff => "/starknet/state_diffs/1",
            Protocol::Transaction => "/starknet/transactions/1",
            Protocol::Class => "/starknet/classes/1",
            Protocol::Event => "/starknet/events/1",
        }
    }
}

impl From<Protocol> for String {
    fn from(protocol: Protocol) -> String {
        protocol.as_str().to_string()
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Unknown protocol: {0}")]
pub struct UnknownProtocolConversionError(String);

lazy_static! {
    static ref PROTOCOL_NAME_TO_PROTOCOL: HashMap<&'static str, Protocol> =
        enum_iterator::all::<Protocol>().map(|protocol| (protocol.as_str(), protocol)).collect();
}

impl TryFrom<&str> for Protocol {
    type Error = UnknownProtocolConversionError;

    fn try_from(protocol: &str) -> Result<Self, Self::Error> {
        PROTOCOL_NAME_TO_PROTOCOL
            .get(protocol)
            .copied()
            .ok_or_else(|| UnknownProtocolConversionError(protocol.to_string()))
    }
}

impl TryFrom<String> for Protocol {
    type Error = UnknownProtocolConversionError;

    fn try_from(protocol: String) -> Result<Self, Self::Error> {
        Protocol::try_from(protocol.as_str())
    }
}
