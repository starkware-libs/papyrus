use std::io::ErrorKind;

use assert_matches::assert_matches;
use futures::AsyncWriteExt;
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::swarm::StreamProtocol;
use pretty_assertions::assert_eq;

use super::{InboundProtocol, OutboundProtocol};
use crate::messages::block::{BlockHeadersRequest, BlockHeadersResponse};
use crate::messages::{read_message, write_message, write_usize};
use crate::test_utils::{get_connected_streams, hardcoded_data};

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/get_blocks/1.0.0");

#[test]
fn outbound_protocol_info() {
    let outbound_protocol = OutboundProtocol::<BlockHeadersRequest> {
        query: Default::default(),
        protocol_name: PROTOCOL_NAME,
    };
    assert_eq!(outbound_protocol.protocol_info().collect::<Vec<_>>(), vec![PROTOCOL_NAME]);
}

#[test]
fn inbound_protocol_info() {
    let inbound_protocol = InboundProtocol::<BlockHeadersRequest>::new(PROTOCOL_NAME);
    assert_eq!(inbound_protocol.protocol_info().collect::<Vec<_>>(), vec![PROTOCOL_NAME]);
}

#[tokio::test]
async fn positive_flow() {
    let (inbound_stream, outbound_stream, _) = get_connected_streams().await;

    let query = BlockHeadersRequest::default();
    let outbound_protocol = OutboundProtocol { query: query.clone(), protocol_name: PROTOCOL_NAME };
    let inbound_protocol = InboundProtocol::<BlockHeadersRequest>::new(PROTOCOL_NAME);

    tokio::join!(
        async move {
            let (received_query, mut stream) =
                inbound_protocol.upgrade_inbound(inbound_stream, PROTOCOL_NAME).await.unwrap();
            assert_eq!(query, received_query);
            for response in hardcoded_data() {
                write_message(response, &mut stream).await.unwrap();
            }
        },
        async move {
            let mut stream =
                outbound_protocol.upgrade_outbound(outbound_stream, PROTOCOL_NAME).await.unwrap();
            for expected_response in hardcoded_data() {
                let response =
                    read_message::<BlockHeadersResponse, _>(&mut stream).await.unwrap().unwrap();
                assert_eq!(response, expected_response);
            }
        }
    );
}

#[tokio::test]
async fn outbound_sends_invalid_request() {
    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let inbound_protocol = InboundProtocol::<BlockHeadersRequest>::new(PROTOCOL_NAME);

    tokio::join!(
        async move {
            assert!(inbound_protocol.upgrade_inbound(inbound_stream, PROTOCOL_NAME).await.is_err());
        },
        async move {
            // The first element is the length of the message, if we don't write that many bytes
            // after then the message will be invalid.
            write_usize(&mut outbound_stream, 10).await.unwrap();
            outbound_stream.close().await.unwrap();
        },
    );
}

#[tokio::test]
async fn outbound_sends_no_request() {
    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let inbound_protocol = InboundProtocol::<BlockHeadersRequest>::new(PROTOCOL_NAME);

    tokio::join!(
        async move {
            let error =
                inbound_protocol.upgrade_inbound(inbound_stream, PROTOCOL_NAME).await.unwrap_err();
            assert_matches!(error.kind(), ErrorKind::UnexpectedEof);
        },
        async move {
            outbound_stream.close().await.unwrap();
        },
    );
}
