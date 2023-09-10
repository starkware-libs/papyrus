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
use prost::Message;

use super::super::handler::NewQueryEvent;
use super::super::protocol::PROTOCOL_NAME;
use super::super::OutboundSessionId;
use super::{Behaviour, Event};
use crate::messages::block::{GetBlocks, GetBlocksResponse};

pub struct GetBlocksPollParameters {}

impl PollParameters for GetBlocksPollParameters {
    type SupportedProtocolsIter = iter::Once<Vec<u8>>;
    fn supported_protocols(&self) -> Self::SupportedProtocolsIter {
        iter::once(PROTOCOL_NAME.as_ref().as_bytes().to_vec())
    }
}

impl<Query: Message + Clone, Data: Message> Unpin for Behaviour<Query, Data> {}

impl<Query: Message + Clone + 'static, Data: Message + Default + 'static> Stream
    for Behaviour<Query, Data>
{
    type Item = ToSwarm<Event<Query, Data>, NewQueryEvent<Query>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx, &mut GetBlocksPollParameters {}) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

fn validate_no_events<Query: Message + Clone + 'static, Data: Message + Default + 'static>(
    behaviour: &mut Behaviour<Query, Data>,
) {
    assert!(behaviour.next().now_or_never().is_none());
}

async fn validate_next_event_dial<
    Query: Message + Clone + 'static,
    Data: Message + Default + 'static,
>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
) {
    let event = behaviour.next().await.unwrap();
    let ToSwarm::Dial { opts } = event else {
        panic!("Got unexpected event");
    };
    assert_eq!(*peer_id, opts.get_peer_id().unwrap());
}

async fn validate_next_event_send_query_to_handler<
    Query: Message + Clone + PartialEq + 'static,
    Data: Message + Default + 'static,
>(
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
            event: NewQueryEvent::<Query> { query: other_query, outbound_session_id: other_outbound_session_id },
            ..
        } if *peer_id == other_peer_id
            && *outbound_session_id == other_outbound_session_id
            && *query == other_query
    );
}

#[tokio::test]
async fn send_and_process_request() {
    let mut behaviour = Behaviour::<GetBlocks, GetBlocksResponse>::new(SUBSTREAM_TIMEOUT);

    let query = GetBlocks::default();
    let peer_id = PeerId::random();

    let outbound_session_id = behaviour.send_query(query.clone(), peer_id);
    validate_next_event_dial(&mut behaviour, &peer_id).await;
    validate_no_events(&mut behaviour);

    let connection_id = ConnectionId::new_unchecked(0);
    let address = Multiaddr::empty();
    let role_override = Endpoint::Dialer;
    let _handler = behaviour
        .handle_established_outbound_connection(connection_id, peer_id, &address, role_override)
        .unwrap();
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id,
        endpoint: &ConnectedPoint::Dialer { address, role_override },
        failed_addresses: &[],
        other_established: 0,
    }));
    validate_next_event_send_query_to_handler(
        &mut behaviour,
        &peer_id,
        &query,
        &outbound_session_id,
    )
    .await;
    validate_no_events(&mut behaviour);

    // TODO(shahak): Send responses from the handler.
}
