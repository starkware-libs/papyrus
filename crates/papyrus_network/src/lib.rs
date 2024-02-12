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
use std::time::Duration;
use std::usize;

use futures::channel::mpsc::{Receiver, Sender};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};

pub struct NetworkConfig {
    pub listen_addresses: Vec<String>,
    pub session_timeout: Duration,
    pub idle_connection_timeout: Duration,
    pub header_buffer_size: usize,
}

#[derive(Default)]
pub enum DataType {
    #[default]
    SignedBlock,
}

#[derive(Default)]
pub struct Query {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
    pub data_type: DataType,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct InternalQuery {
    pub start_block: BlockHashOrNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BlockHashOrNumber {
    Hash(BlockHash),
    Number(BlockNumber),
}

#[allow(unused)]
pub struct ResponseReceivers {
    signed_headers_receiver: Receiver<SignedBlockHeader>,
}

#[allow(unused)]
struct ResponseSenders {
    pub signed_headers_sender: Sender<SignedBlockHeader>,
}

impl SerializeConfig for NetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "listen_addresses",
                &self.listen_addresses,
                "The addresses that the peer listens on for incoming connections.",
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
        ])
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/127.0.0.1/udp/10000/quic-v1".to_owned(),
                "/ip4/127.0.0.1/tcp/10001".to_owned(),
            ],
            session_timeout: Duration::from_secs(10),
            idle_connection_timeout: Duration::from_secs(10),
            header_buffer_size: 100000,
        }
    }
}

impl ResponseReceivers {
    fn new(signed_headers_receiver: Receiver<SignedBlockHeader>) -> Self {
        Self { signed_headers_receiver }
    }
}
