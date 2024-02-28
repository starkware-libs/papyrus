use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use prost::Message;
use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockSignature,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    TransactionCommitment,
};
use starknet_api::crypto::Signature;

use crate::db_executor::Data;
use crate::protobuf_messages::protobuf::{self, ConsensusSignature};
use crate::protobuf_messages::{
    enum_int_to_l1_data_availability_mode,
    l1_data_availability_mode_to_enum_int,
    ProtobufConversionError,
};
use crate::{
    BlockHashOrNumber,
    Direction,
    InternalQuery,
    Protocol,
    Query,
    ResponseReceivers,
    SignedBlockHeader,
};

impl ResponseReceivers {
    pub(crate) fn new(mut protocol_to_receiver_map: HashMap<Protocol, Receiver<Vec<u8>>>) -> Self {
        let signed_headers_receiver = protocol_to_receiver_map
            .remove(&Protocol::SignedBlockHeader)
            .expect("SignedBlockHeader receiver not found")
            .map(|data_bytes| {
                protobuf::BlockHeadersResponse::decode(&data_bytes[..])
                    .expect("failed to decode protobuf SignedBlockHeader")
                    .try_into()
                    .expect("failed to convert SignedBlockHeader")
            })
            .boxed();
        Self { signed_headers_receiver }
    }
}

#[allow(unused)]
pub struct Router {
    pub protocol_to_sender_map: HashMap<Protocol, Sender<Vec<u8>>>,
    pub protocol_to_receiver_map: Option<HashMap<Protocol, Receiver<Vec<u8>>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("No sender for protocol {protocol:?}")]
    NoSenderForProtocol { protocol: Protocol },
    #[error(transparent)]
    TrySendError(#[from] futures::channel::mpsc::TrySendError<Vec<u8>>),
}

impl Router {
    pub fn new(protocols: Vec<Protocol>, buffer_size: usize) -> Self {
        let mut protocol_to_sender_map = HashMap::new();
        let mut protocol_to_receiver_map = HashMap::new();
        for protocol in protocols {
            let (sender, receiver) = futures::channel::mpsc::channel(buffer_size);
            protocol_to_sender_map.insert(protocol, sender);
            protocol_to_receiver_map.insert(protocol, receiver);
        }
        Self { protocol_to_sender_map, protocol_to_receiver_map: Some(protocol_to_receiver_map) }
    }

    pub fn get_recievers(&mut self) -> HashMap<Protocol, Receiver<Vec<u8>>> {
        self.protocol_to_receiver_map.take().unwrap_or_default()
    }

    pub fn try_send(&mut self, protocol: Protocol, data: Vec<u8>) -> Result<(), RouterError> {
        self.protocol_to_sender_map
            .get_mut(&protocol)
            .ok_or(RouterError::NoSenderForProtocol { protocol })
            .and_then(|sender| sender.try_send(data).map_err(RouterError::from))
    }
}
