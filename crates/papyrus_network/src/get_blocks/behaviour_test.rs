use std::iter;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use assert_matches::assert_matches;
use futures::{FutureExt, Stream, StreamExt};
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{ConnectionId, FromSwarm, NetworkBehaviour, PollParameters, ToSwarm};
use libp2p::{Multiaddr, PeerId};

use super::super::handler::NewRequestEvent;
use super::super::protocol::PROTOCOL_NAME;
use super::super::RequestId;
use super::{Behaviour, Event};
use crate::messages::block::GetBlocks;

pub struct GetBlocksPollParameters {}

impl PollParameters for GetBlocksPollParameters {
    type SupportedProtocolsIter = iter::Once<Vec<u8>>;
    fn supported_protocols(&self) -> Self::SupportedProtocolsIter {
        iter::once(PROTOCOL_NAME.as_ref().as_bytes().to_vec())
    }
}

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<Event, NewRequestEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx, &mut GetBlocksPollParameters {}) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

fn validate_no_events(behaviour: &mut Behaviour) {
    assert!(behaviour.next().now_or_never().is_none());
}

async fn validate_next_event_dial(behaviour: &mut Behaviour, peer_id: &PeerId) {
    let event = behaviour.next().await.unwrap();
    let ToSwarm::Dial { opts } = event else {
        panic!("Got unexpected event");
    };
    assert_eq!(*peer_id, opts.get_peer_id().unwrap());
}

async fn validate_next_event_send_request_to_handler(
    behaviour: &mut Behaviour,
    peer_id: &PeerId,
    request: &GetBlocks,
    request_id: &RequestId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: other_peer_id,
            event: NewRequestEvent { request: other_request, request_id: other_request_id },
            ..
        } if *peer_id == other_peer_id
            && *request_id == other_request_id
            && *request == other_request
    );
}

#[tokio::test]
async fn send_and_process_request() {
    let mut behaviour = Behaviour::new(SUBSTREAM_TIMEOUT);

    let request = GetBlocks::default();
    let peer_id = PeerId::random();

    let request_id = behaviour.send_request(request.clone(), peer_id.clone());
    validate_next_event_dial(&mut behaviour, &peer_id).await;
    validate_no_events(&mut behaviour);

    let connection_id = ConnectionId::new_unchecked(0);
    let address = Multiaddr::empty();
    let role_override = Endpoint::Dialer;
    let _handler = behaviour
        .handle_established_outbound_connection(
            connection_id.clone(),
            peer_id.clone(),
            &address,
            role_override,
        )
        .unwrap();
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: peer_id.clone(),
        connection_id,
        endpoint: &ConnectedPoint::Dialer { address, role_override },
        failed_addresses: &[],
        other_established: 0,
    }));
    validate_next_event_send_request_to_handler(&mut behaviour, &peer_id, &request, &request_id)
        .await;
    validate_no_events(&mut behaviour);

    // TODO(shahak): Send responses from the handler.
}
