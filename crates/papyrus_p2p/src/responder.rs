use futures::StreamExt;
use futures_channel::mpsc;
use libp2p::request_response::ResponseChannel;
use log::info;
use papyrus_storage::{HeaderStorageReader, StorageReader};
use starknet_api::{BlockHeader, BlockNumber};

use crate::client::Client;
use crate::sync::BlockHeaderResponse;

pub struct Responder {
    storage_reader: StorageReader,
    network_events: mpsc::Receiver<Event>,
    network_client: Client,
}

#[derive(Debug)]
pub enum Event {
    InboundRequest { request: BlockNumber, channel: ResponseChannel<BlockHeaderResponse> },
}
impl Responder {
    pub fn new(
        storage_reader: StorageReader,
        network_events: mpsc::Receiver<Event>,
        network_client: Client,
    ) -> Self {
        Self { storage_reader, network_events, network_client }
    }

    pub async fn run(mut self) {
        info!("Responder is running.");

        loop {
            while let Some(Event::InboundRequest { request, channel }) =
                self.network_events.next().await
            {
                info!("got an InboundRequest {request:?}");
                let res = self.get_block_header(request);
                info!("res is {res:?}");
                self.network_client.respond_block_header(res, channel).await;
            }
        }
    }

    fn get_block_header(&self, block_number: BlockNumber) -> BlockHeader {
        let txn = self.storage_reader.begin_ro_txn().unwrap();
        let header = txn.get_block_header(block_number).unwrap().unwrap();
        header
    }
}
