pub mod client;
pub mod server;

use enum_iterator::Sequence;

pub const BUFFER_SIZE: usize = 100000;

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
            Protocol::SignedBlockHeader => "/starknet/headers/0.1.0-rc.0",
            Protocol::StateDiff => "/starknet/state_diffs/0.1.0-rc.0",
            Protocol::Transaction => "/starknet/transactions/0.1.0-rc.0",
            Protocol::Class => "/starknet/classes/0.1.0-rc.0",
            Protocol::Event => "/starknet/events/0.1.0-rc.0",
        }
    }
}

impl From<Protocol> for String {
    fn from(protocol: Protocol) -> String {
        protocol.as_str().to_string()
    }
}
