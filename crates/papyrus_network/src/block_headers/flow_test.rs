use std::time::Duration;

use assert_matches::assert_matches;
use futures::{FutureExt, StreamExt};
use libp2p::swarm::SwarmEvent;
use starknet_api::block::{BlockHeader, BlockNumber, BlockSignature};

use super::Behaviour;
use crate::block_headers::Event;
use crate::db_executor::Data;
use crate::streamed_data::SessionId;
use crate::test_utils::create_fully_connected_swarms_stream;
use crate::{BlockHashOrNumber, Direction, InternalQuery};

const NUM_OF_BLOCKS: u64 = 10;
const START_BLOCK: u64 = 5;

#[tokio::test]
async fn one_sends_to_the_other() {
    let mut swarms_stream =
        create_fully_connected_swarms_stream(2, || Behaviour::new(Duration::from_secs(5))).await;

    let mut peer_ids = swarms_stream.keys();
    let outbound_peer_id = peer_ids.next().unwrap().clone();
    let inbound_peer_id = peer_ids.next().unwrap().clone();
    drop(peer_ids);

    let mut swarms_mut = swarms_stream.values_mut();
    let outbound_swarm = swarms_mut.next().unwrap();
    let inbound_swarm = swarms_mut.next().unwrap();

    // Side A - send query
    let sent_query = InternalQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(START_BLOCK)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let outbound_session_id = outbound_swarm
        .behaviour_mut()
        .send_query(sent_query, *inbound_swarm.local_peer_id())
        .unwrap();
    outbound_swarm.next().now_or_never();

    // Side B - receive query
    let event = inbound_swarm.next().await.unwrap();
    let inbound_session_id = match event {
        SwarmEvent::Behaviour(Event::NewInboundQuery { query, inbound_session_id }) => {
            assert_eq!(sent_query, query);
            inbound_session_id
        }
        _ => panic!("Unexpected event: {:?}", event),
    };

    // Side B - Send data responding to the query

    for i in START_BLOCK..(START_BLOCK + NUM_OF_BLOCKS) {
        inbound_swarm
            .behaviour_mut()
            .send_data(
                Data::BlockHeaderAndSignature {
                    header: BlockHeader { block_number: BlockNumber(i), ..Default::default() },
                    signature: BlockSignature::default(),
                },
                inbound_session_id,
            )
            .unwrap();
    }

    drop(swarms_mut);

    // Side A - Receive data and validate its correctness.
    for i in START_BLOCK..(START_BLOCK + NUM_OF_BLOCKS) {
        let (peer_id, event) = loop {
            let (peer_id, swarm_event) = swarms_stream.next().await.unwrap();
            let SwarmEvent::Behaviour(event) = swarm_event else {
                continue;
            };
            break (peer_id, event);
        };
        assert_eq!(
            peer_id, outbound_peer_id,
            "Unexpected event from inbound peer while outbound peer waits for data {event:?}"
        );
        assert_matches!(
            event,
            Event::ReceivedData { signed_header, outbound_session_id: event_outbound_session_id }
                if signed_header.block_header.block_number.0 == i &&
                    outbound_session_id == event_outbound_session_id
        );
    }

    // Side B - Send Fin.
    swarms_stream
        .get_mut(&inbound_peer_id)
        .unwrap()
        .behaviour_mut()
        .send_data(Data::Fin, inbound_session_id)
        .unwrap();

    // Side A and B - Wait for SessionFinishedSuccessfully event.
    let mut outbound_finished = false;
    let mut inbound_finished = false;

    for _ in 0..2 {
        let (peer_id, event) = loop {
            let (peer_id, swarm_event) = swarms_stream.next().await.unwrap();
            let SwarmEvent::Behaviour(event) = swarm_event else {
                continue;
            };
            break (peer_id, event);
        };
        match event {
            Event::SessionFinishedSuccessfully {
                session_id: SessionId::OutboundSessionId(event_outbound_session_id),
            } => {
                assert_eq!(outbound_session_id, event_outbound_session_id);
                assert_eq!(peer_id, outbound_peer_id);
                outbound_finished = true;
            }
            Event::SessionFinishedSuccessfully {
                session_id: SessionId::InboundSessionId(event_inbound_session_id),
            } => {
                assert_eq!(inbound_session_id, event_inbound_session_id);
                assert_eq!(peer_id, inbound_peer_id);
                inbound_finished = true;
            }
            _ => panic!("Unexpected event {event:?} while waiting for SessionFinishedSuccessfully"),
        }
    }
    assert!(outbound_finished);
    assert!(inbound_finished);
}
