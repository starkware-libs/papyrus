use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use defaultmap::DefaultHashMap;
use futures::StreamExt;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{ConnectionHandler, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};

use super::behaviour::{Behaviour, Event};
use super::{InboundSessionId, OutboundSessionId, SessionId};
use crate::messages::block::{BlockHeader, GetBlocks, GetBlocksResponse};
use crate::messages::common::BlockId;
use crate::messages::proto::p2p::proto::get_blocks_response::Response;
use crate::test_utils::{create_swarm, StreamHashMap};

const NUM_PEERS: usize = 3;
const NUM_MESSAGES_PER_SESSION: usize = 5;

type SwarmEventAlias<BehaviourTrait> = SwarmEvent<
    <BehaviourTrait as NetworkBehaviour>::ToSwarm,
    <<BehaviourTrait as NetworkBehaviour>::ConnectionHandler as ConnectionHandler>::Error,
>;

async fn collect_events_from_swarms<BehaviourTrait: NetworkBehaviour, T>(
    swarms: &mut StreamHashMap<PeerId, Swarm<BehaviourTrait>>,
    mut map_and_filter_event: impl FnMut(PeerId, SwarmEventAlias<BehaviourTrait>) -> Option<(PeerId, T)>,
    assert_unique: bool,
) -> HashMap<(PeerId, PeerId), T> {
    let mut results = HashMap::<(PeerId, PeerId), T>::new();
    loop {
        // Swarms should never finish, so we can unwrap the option.
        let (peer_id, event) = swarms.next().await.unwrap();
        if let Some((other_peer_id, value)) = map_and_filter_event(peer_id, event) {
            let is_unique = results.insert((peer_id, other_peer_id), value).is_none();
            if assert_unique {
                assert!(is_unique);
            }
            if results.len() == (NUM_PEERS - 1) * NUM_PEERS {
                break;
            }
        }
    }
    results
}

fn perform_action_on_swarms<BehaviourTrait: NetworkBehaviour>(
    swarms: &mut StreamHashMap<PeerId, Swarm<BehaviourTrait>>,
    peer_ids_and_addresses: &Vec<(PeerId, Multiaddr)>,
    action: &mut dyn FnMut(&mut Swarm<BehaviourTrait>, PeerId, Multiaddr),
) {
    for swarm in swarms.values_mut() {
        let peer_id = *swarm.local_peer_id();
        for (other_peer_id, other_address) in peer_ids_and_addresses.iter().cloned() {
            if peer_id == other_peer_id {
                continue;
            }
            action(swarm, other_peer_id, other_address);
        }
    }
}

fn dial<BehaviourTrait: NetworkBehaviour>(
    swarm: &mut Swarm<BehaviourTrait>,
    other_peer_id: PeerId,
    other_address: Multiaddr,
) {
    swarm.dial(DialOpts::peer_id(other_peer_id).addresses(vec![other_address]).build()).unwrap();
}

fn send_query_and_update_map(
    outbound_swarm: &mut Swarm<Behaviour<GetBlocks, GetBlocksResponse>>,
    inbound_peer_id: PeerId,
    outbound_session_id_to_peer_id: &mut HashMap<(PeerId, OutboundSessionId), PeerId>,
) {
    let outbound_peer_id = *outbound_swarm.local_peer_id();
    let outbound_session_id = outbound_swarm
        .behaviour_mut()
        .send_query(
            GetBlocks {
                step: get_number_for_query(outbound_peer_id, inbound_peer_id),
                ..Default::default()
            },
            inbound_peer_id,
        )
        .unwrap();
    outbound_session_id_to_peer_id.insert((outbound_peer_id, outbound_session_id), inbound_peer_id);
}

fn send_data(
    inbound_swarm: &mut Swarm<Behaviour<GetBlocks, GetBlocksResponse>>,
    outbound_peer_id: PeerId,
    inbound_session_ids: &HashMap<(PeerId, PeerId), InboundSessionId>,
) {
    let inbound_peer_id = *inbound_swarm.local_peer_id();
    for i in 0..NUM_MESSAGES_PER_SESSION {
        inbound_swarm
            .behaviour_mut()
            .send_data(
                GetBlocksResponse {
                    response: Some(Response::Header(BlockHeader {
                        parent_block: Some(BlockId {
                            hash: None,
                            height: get_number_for_data(inbound_peer_id, outbound_peer_id, i),
                        }),
                        ..Default::default()
                    })),
                },
                inbound_session_ids[&(inbound_peer_id, outbound_peer_id)],
            )
            .unwrap();
    }
}

fn close_inbound_session(
    inbound_swarm: &mut Swarm<Behaviour<GetBlocks, GetBlocksResponse>>,
    outbound_peer_id: PeerId,
    inbound_session_ids: &HashMap<(PeerId, PeerId), InboundSessionId>,
) {
    let inbound_peer_id = *inbound_swarm.local_peer_id();
    inbound_swarm
        .behaviour_mut()
        .close_session(SessionId::InboundSessionId(
            inbound_session_ids[&(inbound_peer_id, outbound_peer_id)],
        ))
        .unwrap();
}

fn check_connection_established_event(
    _: PeerId,
    swarm_event: SwarmEventAlias<Behaviour<GetBlocks, GetBlocksResponse>>,
) -> Option<(PeerId, ())> {
    let SwarmEvent::ConnectionEstablished { peer_id, .. } = swarm_event else {
        return None;
    };
    Some((peer_id, ()))
}

fn check_new_inbound_session_event_and_return_id(
    inbound_peer_id: PeerId,
    swarm_event: SwarmEventAlias<Behaviour<GetBlocks, GetBlocksResponse>>,
) -> Option<(PeerId, InboundSessionId)> {
    let SwarmEvent::Behaviour(event) = swarm_event else {
        return None;
    };
    let Event::NewInboundSession { query, inbound_session_id, peer_id: outbound_peer_id } = event
    else {
        panic!("Got unexpected event {:?} when expecting NewInboundSession", event);
    };
    assert_eq!(query.step, get_number_for_query(outbound_peer_id, inbound_peer_id));
    Some((outbound_peer_id, inbound_session_id))
}

fn check_received_data_event(
    outbound_peer_id: PeerId,
    swarm_event: SwarmEventAlias<Behaviour<GetBlocks, GetBlocksResponse>>,
    current_message: &mut DefaultHashMap<(PeerId, PeerId), usize>,
    outbound_session_id_to_peer_id: &HashMap<(PeerId, OutboundSessionId), PeerId>,
) -> Option<(PeerId, ())> {
    let SwarmEvent::Behaviour(event) = swarm_event else {
        return None;
    };
    let Event::ReceivedData { outbound_session_id, data } = event else {
        panic!("Got unexpected event {:?} when expecting ReceivedData", event);
    };
    let inbound_peer_id = outbound_session_id_to_peer_id[&(outbound_peer_id, outbound_session_id)];
    let GetBlocksResponse {
        response:
            Some(Response::Header(BlockHeader { parent_block: Some(BlockId { height, .. }), .. })),
    } = data
    else {
        panic!("Got unexpected data {:?}", data);
    };
    let message_index = *current_message.get((outbound_peer_id, inbound_peer_id));
    assert_eq!(height, get_number_for_data(inbound_peer_id, outbound_peer_id, message_index));
    current_message.insert((outbound_peer_id, inbound_peer_id), message_index + 1);
    Some((inbound_peer_id, ()))
}

fn check_outbound_session_closed_by_peer_event(
    peer_id: PeerId,
    swarm_event: SwarmEventAlias<Behaviour<GetBlocks, GetBlocksResponse>>,
    outbound_session_id_to_peer_id: &HashMap<(PeerId, OutboundSessionId), PeerId>,
) -> Option<(PeerId, ())> {
    let SwarmEvent::Behaviour(Event::SessionClosedByPeer {
        session_id: SessionId::OutboundSessionId(outbound_session_id),
        ..
    }) = swarm_event
    else {
        return None;
    };
    Some((outbound_session_id_to_peer_id[&(peer_id, outbound_session_id)], ()))
}

fn get_number_for_query(peer_id1: PeerId, peer_id2: PeerId) -> u64 {
    let mut hasher = DefaultHasher::new();
    peer_id1.hash(&mut hasher);
    peer_id2.hash(&mut hasher);
    hasher.finish()
}

fn get_number_for_data(peer_id1: PeerId, peer_id2: PeerId, message_index: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    peer_id1.hash(&mut hasher);
    peer_id2.hash(&mut hasher);
    message_index.hash(&mut hasher);
    hasher.finish()
}

#[tokio::test]
async fn everyone_sends_to_everyone() {
    let substream_timeout = Duration::from_secs(3600);

    let swarms_and_addresses = (0..NUM_PEERS)
        .map(|_| create_swarm(Behaviour::<GetBlocks, GetBlocksResponse>::new(substream_timeout)))
        .collect::<Vec<_>>();
    let peer_ids_and_addresses = swarms_and_addresses
        .iter()
        .map(|(swarm, address)| (*swarm.local_peer_id(), address.clone()))
        .collect::<Vec<_>>();

    // Collect swarms to a single stream
    let mut swarms = StreamHashMap::new(
        swarms_and_addresses
            .into_iter()
            .map(|(swarm, _)| (*swarm.local_peer_id(), swarm))
            .collect(),
    );

    perform_action_on_swarms(&mut swarms, &peer_ids_and_addresses, &mut dial);

    collect_events_from_swarms(&mut swarms, check_connection_established_event, false).await;

    let mut outbound_session_id_to_peer_id = HashMap::<(PeerId, OutboundSessionId), PeerId>::new();
    perform_action_on_swarms(
        &mut swarms,
        &peer_ids_and_addresses,
        &mut |outbound_swarm, inbound_peer_id, _| {
            send_query_and_update_map(
                outbound_swarm,
                inbound_peer_id,
                &mut outbound_session_id_to_peer_id,
            )
        },
    );

    let inbound_session_ids = collect_events_from_swarms(
        &mut swarms,
        check_new_inbound_session_event_and_return_id,
        true,
    )
    .await;

    perform_action_on_swarms(
        &mut swarms,
        &peer_ids_and_addresses,
        &mut |inbound_swarm, outbound_peer_id, _| {
            send_data(inbound_swarm, outbound_peer_id, &inbound_session_ids);
        },
    );

    let mut current_message = DefaultHashMap::<(PeerId, PeerId), usize>::new();
    collect_events_from_swarms(
        &mut swarms,
        |peer_id, event| {
            check_received_data_event(
                peer_id,
                event,
                &mut current_message,
                &outbound_session_id_to_peer_id,
            )
        },
        false,
    )
    .await;

    // TODO(shahak): Create a test where the outbound closes the session, and use the code below
    // in it.
    // let peer_id_to_outbound_session_id = outbound_session_id_to_peer_id
    //     .iter()
    //     .map(|((outbound_peer_id, outbound_session_id), inbound_peer_id)| {
    //         ((*outbound_peer_id, *inbound_peer_id), *outbound_session_id)
    //     })
    //     .collect::<HashMap<_, _>>();
    perform_action_on_swarms(
        &mut swarms,
        &peer_ids_and_addresses,
        &mut |outbound_swarm, inbound_peer_id, _| {
            close_inbound_session(outbound_swarm, inbound_peer_id, &inbound_session_ids)
        },
    );

    collect_events_from_swarms(
        &mut swarms,
        |peer_id, event| {
            check_outbound_session_closed_by_peer_event(
                peer_id,
                event,
                &outbound_session_id_to_peer_id,
            )
        },
        false,
    )
    .await;
}
