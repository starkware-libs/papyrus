use std::time::Duration;

use assert_matches::assert_matches;
use futures::{select, FutureExt, StreamExt};
use libp2p::swarm::SwarmEvent;
use libp2p::StreamProtocol;
use starknet_api::block::BlockNumber;

use super::Behaviour;
use crate::block_headers::Event;
use crate::db_executor::{self, DBExecutor};
use crate::streamed_data::Config;
use crate::test_utils::create_fully_connected_swarms_stream;
use crate::{BlockQuery, Direction};

#[tokio::test]
async fn one_sends_to_the_other() {
    let mut db_executor = db_executor::dummy_executor::DummyDBExecutor::new();
    let mut swarms_stream = create_fully_connected_swarms_stream(2, || {
        Behaviour::new(Config {
            substream_timeout: Duration::from_secs(60),
            protocol_name: StreamProtocol::new("/"),
        })
    })
    .await;

    let mut swarms_mut = swarms_stream.values_mut();
    let outbound_swarm = swarms_mut.next().unwrap();
    let inbound_swarm = swarms_mut.next().unwrap();

    // Side A - send query
    let number_of_blocks = 10;
    let sent_query = BlockQuery {
        start_block: BlockNumber(1),
        direction: Direction::Forward,
        limit: number_of_blocks,
        step: 1,
    };
    let outbound_session_id = outbound_swarm
        .behaviour_mut()
        .send_query(sent_query, *inbound_swarm.local_peer_id())
        .unwrap();
    outbound_swarm.next().now_or_never();

    // Side B - receive query
    let event = inbound_swarm.next().await.unwrap();
    let (inbound_session_id, received_query) = match event {
        SwarmEvent::Behaviour(Event::NewInboundQuery { query, inbound_session_id }) => {
            assert_eq!(sent_query, query);
            (inbound_session_id, query)
        }
        _ => panic!("Unexpected event: {:?}", event),
    };

    // Side B - register query
    let query_id = db_executor.register_query(received_query);

    let mut data_counter = 0;
    loop {
        select! {
            // Side B - poll DB and instruct behaviour to send data
            res = db_executor.next().fuse() => {
                if let Some((curr_query_id, data)) = res {
                    assert_eq!(query_id, curr_query_id);
                    inbound_swarm.behaviour_mut().send_data(data, inbound_session_id).unwrap();
                }
            },
            // Side B - poll to perform data sending
            event = inbound_swarm.next().fuse() => {
                let event = event.unwrap();
                match event {
                    SwarmEvent::Behaviour(Event::SessionCompletedSuccessfully { .. }) => {
                        break;
                    },
                    _ => panic!("Unexpected event: {:?}", event),
                };
            },
            // Side A - receive data
            event = outbound_swarm.next().fuse() => {
                let event = event.unwrap();
                match event {
                    SwarmEvent::Behaviour(Event::ReceivedData {
                        data: _,
                        outbound_session_id: cur_outbound_session_id,
                    }) => {
                        assert_eq!(outbound_session_id, cur_outbound_session_id);
                        data_counter += 1;
                    },
                    SwarmEvent::Behaviour(Event::SessionFailed { session_error, .. }) => {
                        assert_matches!(session_error, super::SessionError::ReceivedFin);
                        assert_eq!(data_counter, number_of_blocks);
                        data_counter += 1;
                    },
                    SwarmEvent::Behaviour(Event::SessionCompletedSuccessfully { .. }) => {
                        // Once all data is sent the inbound session is closed.
                        assert_eq!(data_counter, number_of_blocks+1);
                        break;
                    },
                    _ => panic!("Unexpected event: {:?}", event),
                };
            },
            complete => break,
        }
    }
}
