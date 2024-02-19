use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::swarm::StreamProtocol;
use pretty_assertions::assert_eq;

use super::super::messages::{read_message, write_message};
use super::{InboundProtocol, OutboundProtocol};
use crate::test_utils::{dummy_data, get_connected_streams};

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/example/1.0.0");

#[test]
fn outbound_protocol_info() {
    let outbound_protocol =
        OutboundProtocol { query: Default::default(), protocol_name: PROTOCOL_NAME };
    assert_eq!(outbound_protocol.protocol_info().collect::<Vec<_>>(), vec![PROTOCOL_NAME]);
}

#[test]
fn inbound_protocol_info() {
    let protocol_names = vec![PROTOCOL_NAME, StreamProtocol::new("/example/2.0.0")];
    let inbound_protocol = InboundProtocol::new(protocol_names.clone());
    assert_eq!(inbound_protocol.protocol_info(), protocol_names);
}

#[tokio::test]
async fn positive_flow() {
    let (inbound_stream, outbound_stream, _) = get_connected_streams().await;

    let query = vec![1u8, 2u8, 3u8];
    let outbound_protocol = OutboundProtocol { query: query.clone(), protocol_name: PROTOCOL_NAME };
    let inbound_protocol = InboundProtocol::new(vec![PROTOCOL_NAME]);

    tokio::join!(
        async move {
            let (received_query, mut stream, protocol_name) =
                inbound_protocol.upgrade_inbound(inbound_stream, PROTOCOL_NAME).await.unwrap();
            assert_eq!(query, received_query);
            assert_eq!(protocol_name, PROTOCOL_NAME);
            for response in dummy_data() {
                write_message(&response, &mut stream).await.unwrap();
            }
        },
        async move {
            let mut stream =
                outbound_protocol.upgrade_outbound(outbound_stream, PROTOCOL_NAME).await.unwrap();
            for expected_response in dummy_data() {
                let response = read_message(&mut stream).await.unwrap().unwrap();
                assert_eq!(response, expected_response);
            }
        }
    );
}

#[tokio::test]
async fn inbound_dropped() {
    let (inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let outbound_protocol = OutboundProtocol { query: vec![0u8], protocol_name: PROTOCOL_NAME };

    drop(inbound_stream);

    // Need to sleep to make sure the dropping occurs on the other stream.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    assert!(outbound_protocol.upgrade_outbound(outbound_stream, PROTOCOL_NAME).await.is_err());
}
