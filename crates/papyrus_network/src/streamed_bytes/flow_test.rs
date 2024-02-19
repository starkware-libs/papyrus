use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use defaultmap::DefaultHashMap;
use futures::StreamExt;
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent};
use libp2p::{PeerId, Swarm};

use super::behaviour::{Behaviour, Event};
use super::{Bytes, Config, InboundSessionId, OutboundSessionId, SessionId};
use crate::test_utils::{create_fully_connected_swarms_stream, StreamHashMap};

const NUM_PEERS: usize = 3;
const NUM_MESSAGES_PER_SESSION: usize = 5;

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/example");
pub const OTHER_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/other");

type SwarmEventAlias<BehaviourTrait> = SwarmEvent<<BehaviourTrait as NetworkBehaviour>::ToSwarm>;

async fn collect_events_from_swarms<BehaviourTrait: NetworkBehaviour, T>(
    swarms_stream: &mut StreamHashMap<PeerId, Swarm<BehaviourTrait>>,
    mut map_and_filter_event: impl FnMut(PeerId, SwarmEventAlias<BehaviourTrait>) -> Option<(PeerId, T)>,
    assert_unique: bool,
) -> HashMap<(PeerId, PeerId), T> {
    let mut results = HashMap::<(PeerId, PeerId), T>::new();
    loop {
        // Swarms should never finish, so we can unwrap the option.
        let (peer_id, event) = swarms_stream.next().await.unwrap();
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
    swarms_stream: &mut StreamHashMap<PeerId, Swarm<BehaviourTrait>>,
    peer_ids: &[PeerId],
    action: &mut dyn FnMut(&mut Swarm<BehaviourTrait>, PeerId),
) {
    for swarm in swarms_stream.values_mut() {
        let peer_id = *swarm.local_peer_id();
        for other_peer_id in peer_ids.iter().cloned() {
            if peer_id == other_peer_id {
                continue;
            }
            action(swarm, other_peer_id);
        }
    }
}

fn send_query_and_update_map(
    outbound_swarm: &mut Swarm<Behaviour>,
    inbound_peer_id: PeerId,
    outbound_session_id_to_peer_id: &mut HashMap<(PeerId, OutboundSessionId), PeerId>,
) {
    let outbound_peer_id = *outbound_swarm.local_peer_id();
    let outbound_session_id = outbound_swarm
        .behaviour_mut()
        .send_query(
            get_bytes_from_query_indices(outbound_peer_id, inbound_peer_id),
            inbound_peer_id,
            PROTOCOL_NAME,
        )
        .unwrap();
    outbound_session_id_to_peer_id.insert((outbound_peer_id, outbound_session_id), inbound_peer_id);
}

fn send_data(
    inbound_swarm: &mut Swarm<Behaviour>,
    outbound_peer_id: PeerId,
    inbound_session_ids: &HashMap<(PeerId, PeerId), InboundSessionId>,
) {
    let inbound_peer_id = *inbound_swarm.local_peer_id();
    for i in 0..NUM_MESSAGES_PER_SESSION {
        inbound_swarm
            .behaviour_mut()
            .send_data(
                get_bytes_from_data_indices(inbound_peer_id, outbound_peer_id, i),
                inbound_session_ids[&(inbound_peer_id, outbound_peer_id)],
            )
            .unwrap();
    }
}

fn close_inbound_session(
    inbound_swarm: &mut Swarm<Behaviour>,
    outbound_peer_id: PeerId,
    inbound_session_ids: &HashMap<(PeerId, PeerId), InboundSessionId>,
) {
    let inbound_peer_id = *inbound_swarm.local_peer_id();
    inbound_swarm
        .behaviour_mut()
        .close_inbound_session(inbound_session_ids[&(inbound_peer_id, outbound_peer_id)])
        .unwrap();
}

fn check_new_inbound_session_event_and_return_id(
    inbound_peer_id: PeerId,
    swarm_event: SwarmEventAlias<Behaviour>,
) -> Option<(PeerId, InboundSessionId)> {
    let SwarmEvent::Behaviour(event) = swarm_event else {
        return None;
    };
    let Event::NewInboundSession {
        query,
        inbound_session_id,
        peer_id: outbound_peer_id,
        protocol_name,
    } = event
    else {
        panic!("Got unexpected event {:?} when expecting NewInboundSession", event);
    };
    assert_eq!(query, get_bytes_from_query_indices(outbound_peer_id, inbound_peer_id));
    assert_eq!(protocol_name, PROTOCOL_NAME);
    Some((outbound_peer_id, inbound_session_id))
}

fn check_received_data_event(
    outbound_peer_id: PeerId,
    swarm_event: SwarmEventAlias<Behaviour>,
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
    let message_index = *current_message.get((outbound_peer_id, inbound_peer_id));
    assert_eq!(data, get_bytes_from_data_indices(inbound_peer_id, outbound_peer_id, message_index),);
    current_message.insert((outbound_peer_id, inbound_peer_id), message_index + 1);
    Some((inbound_peer_id, ()))
}

fn check_outbound_session_finished_event(
    peer_id: PeerId,
    swarm_event: SwarmEventAlias<Behaviour>,
    outbound_session_id_to_peer_id: &HashMap<(PeerId, OutboundSessionId), PeerId>,
) -> Option<(PeerId, ())> {
    let SwarmEvent::Behaviour(Event::SessionFinishedSuccessfully {
        session_id: SessionId::OutboundSessionId(outbound_session_id),
        ..
    }) = swarm_event
    else {
        return None;
    };
    Some((outbound_session_id_to_peer_id[&(peer_id, outbound_session_id)], ()))
}

fn get_bytes_from_query_indices(peer_id1: PeerId, peer_id2: PeerId) -> Bytes {
    let mut hasher = DefaultHasher::new();
    peer_id1.hash(&mut hasher);
    peer_id2.hash(&mut hasher);
    hasher.finish().to_be_bytes().to_vec()
}

fn get_bytes_from_data_indices(peer_id1: PeerId, peer_id2: PeerId, message_index: usize) -> Bytes {
    let mut hasher = DefaultHasher::new();
    peer_id1.hash(&mut hasher);
    peer_id2.hash(&mut hasher);
    message_index.hash(&mut hasher);
    hasher.finish().to_be_bytes().to_vec()
}

#[tokio::test]
async fn everyone_sends_to_everyone() {
    let mut swarms_stream = create_fully_connected_swarms_stream(NUM_PEERS, || {
        Behaviour::new(Config {
            session_timeout: Duration::from_secs(5),
            supported_inbound_protocols: vec![PROTOCOL_NAME, OTHER_PROTOCOL_NAME],
        })
    })
    .await;

    let peer_ids = swarms_stream.keys().copied().collect::<Vec<_>>();

    let mut outbound_session_id_to_peer_id = HashMap::<(PeerId, OutboundSessionId), PeerId>::new();
    perform_action_on_swarms(
        &mut swarms_stream,
        &peer_ids,
        &mut |outbound_swarm, inbound_peer_id| {
            send_query_and_update_map(
                outbound_swarm,
                inbound_peer_id,
                &mut outbound_session_id_to_peer_id,
            )
        },
    );

    let inbound_session_ids = collect_events_from_swarms(
        &mut swarms_stream,
        check_new_inbound_session_event_and_return_id,
        true,
    )
    .await;

    perform_action_on_swarms(
        &mut swarms_stream,
        &peer_ids,
        &mut |inbound_swarm, outbound_peer_id| {
            send_data(inbound_swarm, outbound_peer_id, &inbound_session_ids);
        },
    );

    let mut current_message = DefaultHashMap::<(PeerId, PeerId), usize>::new(0);
    collect_events_from_swarms(
        &mut swarms_stream,
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
        &mut swarms_stream,
        &peer_ids,
        &mut |outbound_swarm, inbound_peer_id| {
            close_inbound_session(outbound_swarm, inbound_peer_id, &inbound_session_ids)
        },
    );

    collect_events_from_swarms(
        &mut swarms_stream,
        |peer_id, event| {
            check_outbound_session_finished_event(peer_id, event, &outbound_session_id_to_peer_id)
        },
        false,
    )
    .await;
}
