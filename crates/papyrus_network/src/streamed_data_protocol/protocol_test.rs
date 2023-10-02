use std::io::ErrorKind;

use assert_matches::assert_matches;
use futures::AsyncWriteExt;
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade};
use libp2p::core::UpgradeInfo;
use pretty_assertions::assert_eq;

use super::{InboundProtocol, OutboundProtocol, PROTOCOL_NAME};
use crate::messages::{protobuf, read_message, write_message, write_usize};
use crate::test_utils::{get_connected_streams, hardcoded_data};

#[test]
fn both_protocols_have_same_info() {
    let outbound_protocol =
        OutboundProtocol::<protobuf::BlockHeadersRequest> { query: Default::default() };
    let inbound_protocol = InboundProtocol::<protobuf::BlockHeadersRequest>::new();
    assert_eq!(
        outbound_protocol.protocol_info().collect::<Vec<_>>(),
        inbound_protocol.protocol_info().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn positive_flow() {
    let (inbound_stream, outbound_stream, _) = get_connected_streams().await;

    // TODO(shahak): Change to GetBlocks::default() when the bug that forbids sending default
    // messages is fixed.
    let query = protobuf::BlockHeadersRequest { ..Default::default() };
    let outbound_protocol = OutboundProtocol { query: query.clone() };
    let inbound_protocol = InboundProtocol::<protobuf::BlockHeadersRequest>::new();

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
                let response = read_message::<protobuf::BlockHeadersResponse, _>(&mut stream)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(response, expected_response);
            }
        }
    );
}

#[tokio::test]
async fn outbound_sends_invalid_request() {
    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let inbound_protocol = InboundProtocol::<protobuf::BlockHeadersRequest>::new();

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
    let inbound_protocol = InboundProtocol::<protobuf::BlockHeadersRequest>::new();

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
