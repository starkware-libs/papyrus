pub mod client;
pub mod server;

use enum_iterator::Sequence;

pub const BUFFER_SIZE: usize = 100000;

/// TODO: Support multiple protocols where they're all different versions of the same protocol
/// The p2p sync protocol names needed for negotiation, as they appear in the p2p specs
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Sequence)]
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
