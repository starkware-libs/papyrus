// Simple file exchange protocol
use async_std::io;
use async_trait::async_trait;
use asynchronous_codec::{FramedRead, FramedWrite};
use futures::prelude::*;
use futures::{AsyncRead, AsyncWrite};
use libp2p::request_response::{ProtocolName, RequestResponseCodec};
use log::info;
use prost::Message;
use rand::Rng;
use starknet_api::{BlockHeader, BlockNumber};

use crate::codec::common_proto::{self, FieldElement};

#[derive(Debug, Clone)]
pub struct BlockHeaderExchangeProtocol();
#[derive(Clone)]
pub struct BlockHeaderExchangeCodec();
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeaderRequest {
    pub block_number: BlockNumber,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeaderResponse(pub BlockHeader);

impl ProtocolName for BlockHeaderExchangeProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/sn-sync/headers/1".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for BlockHeaderExchangeCodec {
    type Protocol = BlockHeaderExchangeProtocol;
    type Request = BlockHeaderRequest;
    type Response = BlockHeaderResponse;

    async fn read_request<T>(
        &mut self,
        _: &BlockHeaderExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        info!("read_request");
        let block_number =
            FramedRead::new(io, prost_codec::Codec::<common_proto::BlockNumber>::new(8))
                .next()
                .await
                .ok_or(io::Error::new(io::ErrorKind::Other, "oh no!"))?
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;
        let block_number_del = BlockNumber::new(block_number.number);
        let block_number = BlockNumber::from(block_number);
        assert!(block_number_del == block_number);
        info!("Received: {:?}", block_number);
        Ok(BlockHeaderRequest { block_number })
    }

    async fn read_response<T>(
        &mut self,
        _: &BlockHeaderExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        info!("read_response");
        let block_header =
            FramedRead::new(io, prost_codec::Codec::<common_proto::BlockHeader>::new(1024))
                .next()
                .await
                .ok_or(io::Error::new(io::ErrorKind::Other, "oh no!"))?
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;

        info!("Received: {:?}", block_header);
        let block_header: starknet_api::BlockHeader =
            block_header.try_into().map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;
        info!("Received: {:?}", block_header);

        Ok(BlockHeaderResponse(block_header))
    }

    async fn write_request<T>(
        &mut self,
        _: &BlockHeaderExchangeProtocol,
        io: &mut T,
        BlockHeaderRequest { block_number }: BlockHeaderRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        info!("write_request");
        let message_to_del = common_proto::BlockNumber { number: block_number.number().to_owned() };
        let message = common_proto::BlockNumber::from(block_number);
        assert!(message_to_del == message);

        let mut framed_io =
            FramedWrite::new(io, prost_codec::Codec::<common_proto::BlockNumber>::new(8));

        framed_io
            .send(message)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;
        framed_io.close().await.map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &BlockHeaderExchangeProtocol,
        io: &mut T,
        BlockHeaderResponse(block_header): BlockHeaderResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        info!("write_response");
        // number: block_number.number().to_owned()
        let _def_res = BlockHeader::default();
        let message = common_proto::BlockHeader::from(block_header);
        let _message_1 = common_proto::BlockHeader {
            parent_block_hash: Some(FieldElement {
                element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
            }),
            block_number: rand::thread_rng().gen::<u64>(),
            global_state_root: Some(FieldElement {
                element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
            }),
            sequencer_address: Some(FieldElement {
                element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
            }),
            block_timestamp: rand::thread_rng().gen::<u64>(),
            // transaction_count: rand::thread_rng().gen::<u32>(),
            // transaction_commitment: Some(FieldElement {
            //     element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
            // }),

            // event_count: rand::thread_rng().gen::<u32>(),
            // event_commitment: Some(FieldElement {
            //     element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
            // }),

            // protocol_version: rand::thread_rng().gen::<u32>(),
        };
        // assert_eq!(message, message_1);
        let mut framed_io =
            FramedWrite::new(io, prost_codec::Codec::<common_proto::BlockHeader>::new(1024));
        let encoded_len = message.encoded_len();
        info!("message.encoded_len() {encoded_len}");
        framed_io
            .send(message)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;
        framed_io.close().await.map_err(|_| io::Error::new(io::ErrorKind::Other, "oh no!"))?;

        Ok(())
    }
}
