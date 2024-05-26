pub mod protobuf_conversion;

use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use prost::Message;

use crate::protobuf_messages::protobuf::{self};
use crate::{Protocol, ResponseReceivers};

impl ResponseReceivers {
    pub(crate) fn new(mut protocol_to_receiver_map: HashMap<Protocol, Receiver<Vec<u8>>>) -> Self {
        // TODO: instead of panicing return a stream of results with an error for the subscriber can
        // decide how to proceed.
        let signed_headers_receiver =
            protocol_to_receiver_map.remove(&Protocol::SignedBlockHeader).map(|receiver| {
                receiver
                    .map(|data_bytes| {
                        protobuf::BlockHeadersResponse::decode(&data_bytes[..])
                            .expect("failed to decode protobuf SignedBlockHeader")
                            .try_into()
                            .expect("failed to convert SignedBlockHeader")
                    })
                    .boxed()
            });
        let state_diffs_receiver =
            protocol_to_receiver_map.remove(&Protocol::StateDiff).map(|receiver| {
                receiver
                    .map(|data_bytes| {
                        protobuf::StateDiffsResponse::decode(&data_bytes[..])
                            .expect("failed to decode protobuf StateDiff")
                            .try_into()
                            .expect("failed to convert ThinStateDiff")
                    })
                    .boxed()
            });
        Self { signed_headers_receiver, state_diffs_receiver }
    }
}

#[allow(unused)]
pub(crate) struct Router {
    pub protocol_to_sender_map: HashMap<Protocol, Sender<Vec<u8>>>,
    pub protocol_to_receiver_map: Option<HashMap<Protocol, Receiver<Vec<u8>>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("This Router doesn't support protocol {protocol:?}")]
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
