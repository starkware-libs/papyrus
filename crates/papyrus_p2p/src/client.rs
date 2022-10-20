use std::collections::HashSet;
use std::error::Error;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use libp2p::core::PeerId;
use libp2p::request_response::ResponseChannel;
use log::info;
pub use starknet_api::{BlockHeader, BlockNumber};

use crate::sync::BlockHeaderResponse;

#[derive(Clone, Debug)]
pub struct Client {
    pub sender: mpsc::Sender<Request>,
}
impl Client {
    /// Listen for incoming connections on the given address.
    pub async fn get_block_headers(
        &mut self,
        peer: PeerId,
        block_number: BlockNumber,
    ) -> Result<BlockHeader, Box<dyn Error + Send>> {
        info!("get_block_headers");
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Request::RequestBlockHeader { peer, block_number, sender })
            .await
            .expect("Request receiver not to be dropped.");
        let bh = receiver.await.expect("Sender not to be dropped.");
        info!("got block header {bh:?}");
        bh
    }

    /// Find connected peers.
    pub async fn get_peers(&mut self) -> HashSet<PeerId> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Request::GetPeers { sender })
            .await
            .expect("Request receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    /// Respond with the requested block header.
    pub async fn respond_block_header(
        &mut self,
        block_header: BlockHeader,
        channel: ResponseChannel<BlockHeaderResponse>,
    ) {
        info!("respond_block_header");
        self.sender
            .send(Request::RespondBlockHeader { block_header, channel })
            .await
            .expect("Request receiver not to be dropped.");
    }
}

#[derive(Debug)]
pub enum Request {
    RequestBlockHeader {
        peer: PeerId,
        block_number: BlockNumber,
        sender: oneshot::Sender<Result<BlockHeader, Box<dyn Error + Send>>>,
    },
    GetPeers {
        sender: oneshot::Sender<HashSet<PeerId>>,
    },
    RespondBlockHeader {
        block_header: BlockHeader,
        channel: ResponseChannel<BlockHeaderResponse>,
    },
    // RequestFile {
    //     file_name: String,
    //     peer: PeerId,
    //     sender: oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>,
    // },
    // RespondFile {
    //     file: Vec<u8>,
    //     channel: ResponseChannel<crate::FileResponse>,
    // },
}
