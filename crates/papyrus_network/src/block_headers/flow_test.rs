use std::time::Duration;

use futures::{select, FutureExt, StreamExt};
use libp2p::swarm::SwarmEvent;
use starknet_api::block::BlockNumber;

use super::Behaviour;
use crate::block_headers::Event;
use crate::db_executor::{self, DBExecutor};
use crate::test_utils::create_fully_connected_swarms_stream;
use crate::{BlockHashOrNumber, BlockQuery, Direction};

const BUFFER_SIZE: usize = 10;

#[tokio::test]
async fn one_sends_to_the_other() {
    let mut db_executor = db_executor::dummy_executor::DummyDBExecutor::new();
    let mut swarms_stream =
        create_fully_connected_swarms_stream(2, || Behaviour::new(Duration::from_secs(5))).await;

    let mut swarms_mut = swarms_stream.values_mut();
    let outbound_swarm = swarms_mut.next().unwrap();
    let inbound_swarm = swarms_mut.next().unwrap();
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);

    // Side A - send query
    let number_of_blocks = (BUFFER_SIZE - 1) as u64;
    let sent_query = BlockQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(1)),
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
    let _query_id = db_executor.register_query(received_query, sender);

    let mut receiver_stream = receiver.map(|data| (data, inbound_session_id));

    let mut data_counter = 0;
    loop {
        select! {
            // Side B - get data from channel and send to behaviour
            res = receiver_stream.next().fuse() => {
                if let Some((data, inbound_session_id)) = res {
                    inbound_swarm.behaviour_mut().send_data(data, inbound_session_id).unwrap();
                }
            },
            // Side B - poll DB to make sure data is starting to be sent. should not return.
            res = db_executor.next().fuse() => {
                match res {
                    Some(Ok(query_id)) => println!("Query completed successfully. query_id: {:?}", query_id),
                    Some(Err(err)) => panic!("Query failed. error: {:?}", err),
                    None => panic!("DB executor should not return")
                }
            },
            // Side B - poll to perform data sending
            event = inbound_swarm.next().fuse() => {
                let event = event.unwrap();
                match event {
                    SwarmEvent::Behaviour(Event::SessionCompletedSuccessfully { .. }) => {
                        break;
                    },
                    _ => panic!("Inbound - Unexpected event: {:?}", event),
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
                    SwarmEvent::Behaviour(Event::SessionCompletedSuccessfully { .. }) => {
                        // Once all data is sent the inbound session is closed.
                        assert_eq!(data_counter, number_of_blocks);
                        break;
                    },
                    _ => panic!("Outbound - Unexpected event: {:?}", event),
                };
            },
            complete => break,
        }
    }
}
