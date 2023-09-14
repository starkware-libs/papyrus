use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, Future, StreamExt};
use libp2p::core::multiaddr::multiaddr;
use libp2p::core::transport::memory::MemoryTransport;
use libp2p::core::transport::{ListenerId, Transport};
use libp2p::core::upgrade::{write_varint, InboundUpgrade, OutboundUpgrade};
use libp2p::core::UpgradeInfo;
use pretty_assertions::assert_eq;

use super::{InboundProtocol, OutboundProtocol, PROTOCOL_NAME};
use crate::messages::block::{BlockHeader, GetBlocks, GetBlocksResponse};
use crate::messages::common::{BlockId, Fin};
use crate::messages::proto::p2p::proto::get_blocks_response::Response;
use crate::messages::{read_message, write_message};

fn hardcoded_responses() -> Vec<GetBlocksResponse> {
    vec![
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 1 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 2 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 3 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse { response: Some(Response::Fin(Fin {})) },
    ]
}

#[test]
fn both_protocols_have_same_info() {
    let outbound_protocol = OutboundProtocol::<GetBlocks> { query: Default::default() };
    let inbound_protocol = InboundProtocol::<GetBlocks>::new();
    assert_eq!(
        outbound_protocol.protocol_info().collect::<Vec<_>>(),
        inbound_protocol.protocol_info().collect::<Vec<_>>()
    );
}

async fn get_connected_stream_futures() -> (
    impl Future<Output = impl AsyncRead + AsyncWrite>,
    impl Future<Output = impl AsyncRead + AsyncWrite>,
) {
    let address = multiaddr![Memory(0u64)];
    let mut transport = MemoryTransport::new().boxed();
    transport.listen_on(ListenerId::next(), address).unwrap();
    let listener_addr = transport
        .select_next_some()
        .await
        .into_new_address()
        .expect("MemoryTransport not listening on an address!");

    (
        async move {
            let transport_event = transport.next().await.unwrap();
            let (listener_upgrade, _) = transport_event.into_incoming().unwrap();
            listener_upgrade.await.unwrap()
        },
        async move { MemoryTransport::new().dial(listener_addr).unwrap().await.unwrap() },
    )
}

#[tokio::test]
async fn positive_flow() {
    let (inbound_stream_future, outbound_stream_future) = get_connected_stream_futures().await;

    let query = GetBlocks::default();
    let outbound_protocol = OutboundProtocol { query: query.clone() };
    let inbound_protocol = InboundProtocol::<GetBlocks>::new();

    tokio::join!(
        async move {
            let (received_query, mut stream) = inbound_protocol
                .upgrade_inbound(inbound_stream_future.await, PROTOCOL_NAME)
                .await
                .unwrap();
            assert_eq!(query, received_query);
            for response in hardcoded_responses() {
                write_message(response, &mut stream).await.unwrap();
            }
        },
        async move {
            let mut stream = outbound_protocol
                .upgrade_outbound(outbound_stream_future.await, PROTOCOL_NAME)
                .await
                .unwrap();
            for expected_response in hardcoded_responses() {
                let response = read_message::<GetBlocksResponse, _>(&mut stream).await.unwrap();
                assert_eq!(response, expected_response);
            }
        }
    );
}

#[tokio::test]
async fn inbound_closes_stream() {
    let (inbound_stream_future, outbound_stream_future) = get_connected_stream_futures().await;

    let outbound_protocol = OutboundProtocol::<GetBlocks> { query: Default::default() };

    let (_, outbound_stream) = tokio::join!(
        async move {
            let mut inbound_stream = inbound_stream_future.await;
            inbound_stream.close().await.unwrap();
            inbound_stream
        },
        outbound_stream_future,
    );
    assert!(outbound_protocol.upgrade_outbound(outbound_stream, PROTOCOL_NAME).await.is_err());
}

#[tokio::test]
async fn outbound_sends_invalid_request() {
    let (inbound_stream_future, outbound_stream_future) = get_connected_stream_futures().await;
    let inbound_protocol = InboundProtocol::<GetBlocks>::new();

    tokio::join!(
        async move {
            assert!(
                inbound_protocol
                    .upgrade_inbound(inbound_stream_future.await, PROTOCOL_NAME)
                    .await
                    .is_err()
            );
        },
        async move {
            let mut outbound_stream = outbound_stream_future.await;
            // The first element is the length of the message, if we don't write that many bytes
            // after then the message will be invalid.
            write_varint(&mut outbound_stream, 10).await.unwrap();
            outbound_stream.close().await.unwrap();
        },
    );
}
