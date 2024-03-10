pub mod protobuf_conversion;

#[cfg(test)]
mod test;

use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, Sender};
use futures::StreamExt;
use prost::Message;

use crate::protobuf_messages::protobuf::{self};
use crate::{Protocol, QueryId, ResponseReceivers};

impl ResponseReceivers {
    pub(crate) fn new(
        mut protocol_to_receiver_map: HashMap<Protocol, Receiver<(Vec<u8>, QueryId)>>,
    ) -> Self {
        let signed_headers_receiver = protocol_to_receiver_map
            .remove(&Protocol::SignedBlockHeader)
            .expect("SignedBlockHeader receiver not found")
            .map(|(data_bytes, query_id)| {
                (
                    protobuf::BlockHeadersResponse::decode(&data_bytes[..])
                        .expect("failed to decode protobuf SignedBlockHeader")
                        .try_into()
                        .expect("failed to convert SignedBlockHeader"),
                    query_id,
                )
            })
            .boxed();
        Self { signed_headers_receiver }
    }
}

#[allow(unused)]
pub(crate) struct Router {
    pub protocol_to_sender_map: HashMap<Protocol, Sender<(Vec<u8>, QueryId)>>,
    #[allow(clippy::type_complexity)]
    pub protocol_to_receiver_map: Option<HashMap<Protocol, Receiver<(Vec<u8>, QueryId)>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("This Router doesn't support protocol {protocol:?}")]
    NoSenderForProtocol { protocol: Protocol },
    #[error(transparent)]
    TrySendError(#[from] futures::channel::mpsc::TrySendError<(Vec<u8>, QueryId)>),
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

    pub fn get_recievers(&mut self) -> HashMap<Protocol, Receiver<(Vec<u8>, QueryId)>> {
        self.protocol_to_receiver_map.take().unwrap_or_default()
    }

    pub fn try_send(
        &mut self,
        protocol: Protocol,
        data: Vec<u8>,
        query_id: QueryId,
    ) -> Result<(), RouterError> {
        self.protocol_to_sender_map
            .get_mut(&protocol)
            .ok_or(RouterError::NoSenderForProtocol { protocol })
            .and_then(|sender| Ok(sender.try_send((data, query_id))?))
    }
}
