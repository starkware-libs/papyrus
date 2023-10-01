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

use super::super::handler::RequestFromBehaviourEvent;
use super::super::protocol::PROTOCOL_NAME;
use super::super::{DataBound, OutboundSessionId, QueryBound};
use super::{Behaviour, Event};
use crate::messages::block::{BlockHeadersRequest, BlockHeadersResponse};

pub struct GetBlocksPollParameters {}

impl PollParameters for GetBlocksPollParameters {
    type SupportedProtocolsIter = iter::Once<Vec<u8>>;
    fn supported_protocols(&self) -> Self::SupportedProtocolsIter {
        iter::once(PROTOCOL_NAME.as_ref().as_bytes().to_vec())
    }
}

impl<Query: QueryBound, Data: DataBound> Unpin for Behaviour<Query, Data> {}

impl<Query: QueryBound, Data: DataBound> Stream for Behaviour<Query, Data> {
    type Item = ToSwarm<Event<Query, Data>, RequestFromBehaviourEvent<Query, Data>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx, &mut GetBlocksPollParameters {}) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

fn simulate_dial_finished_from_swarm<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
) {
    let connection_id = ConnectionId::new_unchecked(0);
    let address = Multiaddr::empty();
    let role_override = Endpoint::Dialer;
    let _handler = behaviour
        .handle_established_outbound_connection(connection_id, *peer_id, &address, role_override)
        .unwrap();
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: *peer_id,
        connection_id,
        endpoint: &ConnectedPoint::Dialer { address, role_override },
        failed_addresses: &[],
        other_established: 0,
    }));
}

async fn validate_dial_event<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
) {
    let event = behaviour.next().await.unwrap();
    let ToSwarm::Dial { opts } = event else {
        panic!("Got unexpected event");
    };
    assert_eq!(*peer_id, opts.get_peer_id().unwrap());
}

async fn validate_create_outbound_session_event<Query: QueryBound + PartialEq, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
    query: &Query,
    outbound_session_id: &OutboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: other_peer_id,
            event: RequestFromBehaviourEvent::CreateOutboundSession { query: other_query, outbound_session_id: other_outbound_session_id },
            ..
        } if *peer_id == other_peer_id
            && *outbound_session_id == other_outbound_session_id
            && *query == other_query
    );
}

// TODO(shahak): Fix code duplication with handler test.
fn validate_no_events<Query: QueryBound, Data: DataBound>(behaviour: &mut Behaviour<Query, Data>) {
    assert!(behaviour.next().now_or_never().is_none());
}

#[tokio::test]
async fn create_and_process_outbound_session() {
    let mut behaviour =
        Behaviour::<BlockHeadersRequest, BlockHeadersResponse>::new(SUBSTREAM_TIMEOUT);

    // TODO(shahak): Change to BlockHeadersRequest::default() when the bug that forbids sending
    // default messages is fixed.
    let query = BlockHeadersRequest { ..Default::default() };
    let peer_id = PeerId::random();

    let outbound_session_id = behaviour.send_query(query.clone(), peer_id);
    validate_dial_event(&mut behaviour, &peer_id).await;
    validate_no_events(&mut behaviour);

    simulate_dial_finished_from_swarm(&mut behaviour, &peer_id);

    validate_create_outbound_session_event(&mut behaviour, &peer_id, &query, &outbound_session_id)
        .await;
    validate_no_events(&mut behaviour);

    // TODO(shahak): Send responses from the handler.
}
