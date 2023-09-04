use assert_matches::assert_matches;
use futures::{AsyncRead, AsyncWrite, Future, SinkExt, StreamExt};
use libp2p::core::multiaddr::multiaddr;
use libp2p::core::transport::memory::MemoryTransport;
use libp2p::core::transport::{ListenerId, Transport};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade};
use libp2p::core::UpgradeInfo;
use pretty_assertions::assert_eq;

use super::{
    hardcoded_responses,
    InboundProtocol,
    OutboundProtocol,
    OutboundProtocolError,
    PROTOCOL_NAME,
};
use crate::get_blocks::protocol::InboundProtocolError;
use crate::messages::block::{GetSignatures, NewBlock};
use crate::messages::common::BlockId;
use crate::messages::write_message;

#[test]
fn both_protocols_have_same_info() {
    let (outbound_protocol, _) = OutboundProtocol::new(Default::default());
    let (inbound_protocol, _) = InboundProtocol::new();
    assert_eq!(
        outbound_protocol.protocol_info().collect::<Vec<_>>(),
        inbound_protocol.protocol_info().collect::<Vec<_>>()
    );
}

async fn get_connected_io_futures() -> (
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
    let (inbound_io_future, outbound_io_future) = get_connected_io_futures().await;

    let (outbound_protocol, mut responses_receiver) = OutboundProtocol::new(Default::default());
    let (inbound_protocol, (mut request_relay_receiver, response_relay_sender)) =
        InboundProtocol::new();

    tokio::join!(
        async move {
            inbound_protocol.upgrade_inbound(inbound_io_future.await, PROTOCOL_NAME).await.unwrap();
        },
        async move {
            outbound_protocol
                .upgrade_outbound(outbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap();
        },
        // plays the role of the network and DB handlers
        async move {
            // ignore block query for now, just send hardcoded responses
            let _blocks_query = request_relay_receiver.next().await.unwrap();
            request_relay_receiver.close();
            for expected_response in hardcoded_responses() {
                let msg =
                    if expected_response.is_fin() { None } else { Some(expected_response.clone()) };
                match response_relay_sender.unbounded_send(msg) {
                    Ok(_) => {
                        let result = responses_receiver.next().await;
                        if expected_response.is_fin() {
                            assert!(result.is_none());
                            break;
                        } else {
                            assert_eq!(result.unwrap(), expected_response);
                        }
                    }
                    Err(err) => panic!("Failed to send response with err: {err}"),
                }
            }
        }
    );
}

#[tokio::test]
async fn inbound_sends_invalid_response() {
    let (inbound_io_future, outbound_io_future) = get_connected_io_futures().await;

    let (outbound_protocol, mut responses_receiver) = OutboundProtocol::new(Default::default());

    tokio::join!(
        async move {
            let mut inbound_io = inbound_io_future.await;
            write_message(
                NewBlock { id: Some(BlockId { hash: None, height: 1 }) },
                &mut inbound_io,
            )
            .await
            .unwrap();
        },
        async move {
            let err = outbound_protocol
                .upgrade_outbound(outbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap_err();
            assert_matches!(err, OutboundProtocolError::IOError(_));
        },
        async move { assert!(responses_receiver.next().await.is_none()) }
    );
}

#[tokio::test]
async fn outbound_sends_invalid_request() {
    let (inbound_io_future, outbound_io_future) = get_connected_io_futures().await;
    let (inbound_protocol, _) = InboundProtocol::new();

    tokio::join!(
        async move {
            inbound_protocol
                .upgrade_inbound(inbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap_err();
        },
        async move {
            let mut outbound_io = outbound_io_future.await;
            write_message(
                GetSignatures { id: Some(BlockId { hash: None, height: 1 }) },
                &mut outbound_io,
            )
            .await
            .unwrap();
        },
    );
}

#[tokio::test]
async fn outbound_receiver_closed() {
    let (inbound_io_future, outbound_io_future) = get_connected_io_futures().await;

    let (outbound_protocol, mut responses_receiver) = OutboundProtocol::new(Default::default());
    let (inbound_protocol, _) = InboundProtocol::new();
    responses_receiver.close();

    tokio::join!(
        async move {
            inbound_protocol
                .upgrade_inbound(inbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap_err();
        },
        async move {
            let err = outbound_protocol
                .upgrade_outbound(outbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap_err();
            assert_matches!(err, OutboundProtocolError::ResponseSendError(_));
        },
    );
}

#[tokio::test]
async fn response_relay_stops_unexpectedly() {
    let (inbound_io_future, outbound_io_future) = get_connected_io_futures().await;

    let (outbound_protocol, mut responses_receiver) = OutboundProtocol::new(Default::default());
    let (inbound_protocol, (mut request_relay_receiver, mut response_relay_sender)) =
        InboundProtocol::new();

    tokio::join!(
        async move {
            match inbound_protocol
                .upgrade_inbound(inbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap_err()
            {
                InboundProtocolError::IOError(_) => {}
                err => panic!("Unexpected error: {:?}", err),
            };
        },
        async move {
            outbound_protocol
                .upgrade_outbound(outbound_io_future.await, PROTOCOL_NAME)
                .await
                .unwrap_err();
        },
        // plays the role of the network handler
        async move {
            let _blocks_query = request_relay_receiver.next().await.unwrap();
            request_relay_receiver.close();
            let responses = hardcoded_responses();
            match response_relay_sender.unbounded_send(Some(responses[0].clone())) {
                Ok(_) => {
                    let result = responses_receiver.next().await;
                    assert_eq!(result.unwrap(), responses[0]);
                }
                Err(err) => panic!("Failed to send response with err: {err}"),
            }
            let _res = response_relay_sender.close().await;
            assert!(responses_receiver.try_next().is_err());
        }
    );
}
