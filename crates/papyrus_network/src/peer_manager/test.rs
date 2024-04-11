use core::time;

use assert_matches::assert_matches;
use chrono::Duration;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Multiaddr, PeerId};
use mockall::predicate::eq;
use tokio::time::sleep;

use super::behaviour::Event;
use crate::db_executor::QueryId;
use crate::peer_manager::peer::{MockPeerTrait, Peer, PeerTrait};
use crate::peer_manager::{PeerManager, PeerManagerConfig, ReputationModifier};
use crate::streamed_bytes;

#[test]
fn peer_assignment_round_robin() {
    // Create a new peer manager
    let mut peer_manager = PeerManager::new(PeerManagerConfig::default());

    // Add two peers to the peer manager
    let peer1 = Peer::new(PeerId::random(), Multiaddr::empty());
    let peer2 = Peer::new(PeerId::random(), Multiaddr::empty());
    peer_manager.add_peer(peer1.clone());
    peer_manager.add_peer(peer2.clone());

    // Create three queries
    let query1 = QueryId(1);
    let query2 = QueryId(2);
    let query3 = QueryId(3);

    // Assign peers to the queries in a round-robin fashion
    let res1 = peer_manager.assign_peer_to_query(query1);
    let res2 = peer_manager.assign_peer_to_query(query2);
    let res3 = peer_manager.assign_peer_to_query(query3);

    // Verify that the peers are assigned in a round-robin fashion
    let is_peer1_first: bool;
    match res1.unwrap().0 {
        peer_id if peer_id == peer1.peer_id() => {
            is_peer1_first = true;
            assert_eq!(res2.unwrap().0, peer2.peer_id());
            assert_eq!(res3.unwrap().0, peer1.peer_id());
        }
        peer_id if peer_id == peer2.peer_id() => {
            is_peer1_first = false;
            assert_eq!(res2.unwrap().0, peer1.peer_id());
            assert_eq!(res3.unwrap().0, peer2.peer_id());
        }
        peer_id => panic!("Unexpected peer_id: {:?}", peer_id),
    }

    // check assignment events
    for event in peer_manager.pending_events {
        match event {
            Event::NotifyStreamedBytes(
                streamed_bytes::behaviour::InternalEvent::QueryAssigned(query_id, peer_id, _),
            ) => {
                if is_peer1_first {
                    match query_id {
                        QueryId(1) => assert_eq!(peer_id, peer1.peer_id()),
                        QueryId(2) => assert_eq!(peer_id, peer2.peer_id()),
                        QueryId(3) => assert_eq!(peer_id, peer1.peer_id()),
                        _ => panic!("Unexpected query_id: {:?}", query_id),
                    }
                } else {
                    match query_id {
                        QueryId(1) => assert_eq!(peer_id, peer2.peer_id()),
                        QueryId(2) => assert_eq!(peer_id, peer1.peer_id()),
                        QueryId(3) => assert_eq!(peer_id, peer2.peer_id()),
                        _ => panic!("Unexpected query_id: {:?}", query_id),
                    }
                }
            }
        }
    }
}

#[test]
fn peer_assignment_no_peers() {
    // Create a new peer manager
    let mut peer_manager: PeerManager<Peer> = PeerManager::new(PeerManagerConfig::default());

    // Create a query
    let query = QueryId(1);

    // Assign a peer to the query
    assert_matches!(peer_manager.assign_peer_to_query(query), None);
}

#[test]
fn report_peer_calls_update_reputation() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager<MockPeerTrait> = PeerManager::new(config.clone());

    // Create a mock peer
    let (peer, peer_id) = create_mock_peer(config.blacklist_timeout, true);

    // Add the mock peer to the peer manager
    peer_manager.add_peer(peer);

    // Call the report_peer function on the peer manager
    peer_manager.report_peer(peer_id, ReputationModifier::Bad {}).unwrap();
    peer_manager.get_mut_peer(peer_id).unwrap().checkpoint();
}

#[tokio::test]
async fn peer_block_realeased_after_timeout() {
    const DURATION_IN_MILLIS: u64 = 50;
    let mut peer = Peer::new(PeerId::random(), Multiaddr::empty());
    peer.set_timeout_duration(Duration::milliseconds(DURATION_IN_MILLIS as i64));
    peer.update_reputation(ReputationModifier::Bad {});
    assert!(peer.is_blocked());
    sleep(time::Duration::from_millis(DURATION_IN_MILLIS)).await;
    assert!(!peer.is_blocked());
}

#[test]
fn report_peer_on_unknown_peer_id() {
    // Create a new peer manager
    let mut peer_manager: PeerManager<MockPeerTrait> =
        PeerManager::new(PeerManagerConfig::default());

    // report peer on an unknown peer_id
    let peer_id = PeerId::random();
    peer_manager
        .report_peer(peer_id, ReputationModifier::Bad {})
        .expect_err("report_peer on unknown peer_id should return an error");
}

#[test]
fn report_query_calls_update_reputation() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager<MockPeerTrait> = PeerManager::new(config.clone());

    // Create a mock peer
    let (mut peer, peer_id) = create_mock_peer(config.blacklist_timeout, true);
    peer.expect_multiaddr().times(2).return_const(Multiaddr::empty());
    peer.expect_is_blocked().times(1).return_const(false);

    // Add the mock peer to the peer manager
    peer_manager.add_peer(peer);

    // Create a query
    let query_id = QueryId(1);

    // Assign peer to the query
    let (res_peer_id, _) = peer_manager.assign_peer_to_query(query_id).unwrap();
    assert_eq!(res_peer_id, peer_id);

    // Call the report_peer function on the peer manager
    peer_manager.report_query(query_id, ReputationModifier::Bad {}).unwrap();
    peer_manager.get_mut_peer(peer_id).unwrap().checkpoint();
}

#[test]
fn report_query_on_unknown_query_id() {
    // Create a new peer manager
    let mut peer_manager: PeerManager<MockPeerTrait> =
        PeerManager::new(PeerManagerConfig::default());

    // Create a query
    let query_id = QueryId(1);

    peer_manager
        .report_query(query_id, ReputationModifier::Bad {})
        .expect_err("report_query on unknown query_id should return an error");
}

#[test]
fn more_peers_needed() {
    // Create a new peer manager
    let config = PeerManagerConfig { target_num_for_peers: 2, ..Default::default() };
    let mut peer_manager: PeerManager<MockPeerTrait> = PeerManager::new(config.clone());

    // Add a peer to the peer manager
    let (peer1, _peer_id1) = create_mock_peer(config.blacklist_timeout, false);
    peer_manager.add_peer(peer1);

    // assert more peers are needed
    assert!(peer_manager.more_peers_needed());

    // Add another peer to the peer manager
    let (peer2, _peer_id2) = create_mock_peer(config.blacklist_timeout, false);
    peer_manager.add_peer(peer2);

    // assert no more peers are needed
    assert!(!peer_manager.more_peers_needed());
}

#[test]
fn timed_out_peer_not_assignable_to_queries() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager<MockPeerTrait> = PeerManager::new(config.clone());

    // Create a mock peer
    let (mut peer, peer_id) = create_mock_peer(config.blacklist_timeout, true);
    peer.expect_is_blocked().times(1).return_const(true);

    // Add the mock peer to the peer manager
    peer_manager.add_peer(peer);

    // Report the peer as bad
    peer_manager.report_peer(peer_id, ReputationModifier::Bad {}).unwrap();

    // Create a query
    let query_id = QueryId(1);

    // Assign peer to the query
    assert_matches!(peer_manager.assign_peer_to_query(query_id), None);
}

#[test]
fn wrap_around_in_peer_assignment() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager<MockPeerTrait> = PeerManager::new(config.clone());

    // Create a mock peer
    let (mut peer1, peer_id1) = create_mock_peer(config.blacklist_timeout, true);
    peer1.expect_is_blocked().times(..2).return_const(true);

    // Add the mock peer to the peer manager
    peer_manager.add_peer(peer1);

    // Report the peer as bad
    peer_manager.report_peer(peer_id1, ReputationModifier::Bad {}).unwrap();

    // Create a mock peer
    let (mut peer2, peer_id2) = create_mock_peer(config.blacklist_timeout, false);
    peer2.expect_is_blocked().times(2).return_const(false);
    peer2.expect_multiaddr().times(4).return_const(Multiaddr::empty());

    // Add the mock peer to the peer manager
    peer_manager.add_peer(peer2);

    // Create a query
    let query_id = QueryId(1);

    // Assign peer to the query - since we don't know what is the order of the peers in the HashMap,
    // we need to assign twice to make sure we wrap around
    assert_matches!(peer_manager.assign_peer_to_query(query_id), Some((peer_id, _)) if peer_id == peer_id2);
    assert_matches!(peer_manager.assign_peer_to_query(query_id), Some((peer_id, _)) if peer_id == peer_id2);
}

fn create_mock_peer(
    blacklist_timeout_duration: Duration,
    call_update_reputaion: bool,
) -> (MockPeerTrait, PeerId) {
    let peer_id = PeerId::random();
    let mut peer = MockPeerTrait::new();
    let mut mockall_seq = mockall::Sequence::new();

    peer.expect_peer_id().return_const(peer_id);
    peer.expect_set_timeout_duration()
        .times(1)
        .with(eq(blacklist_timeout_duration))
        .return_const(())
        .in_sequence(&mut mockall_seq);
    if call_update_reputaion {
        peer.expect_update_reputation()
            .times(1)
            .with(eq(ReputationModifier::Bad {}))
            .return_once(|_| ())
            .in_sequence(&mut mockall_seq);
    }

    (peer, peer_id)
}

#[test]
fn block_and_allow_inbound_connection() {
    // Create a new peer manager
    let config = PeerManagerConfig::default();
    let mut peer_manager: PeerManager<MockPeerTrait> = PeerManager::new(config.clone());

    // Create a mock peer - blocked
    let (mut peer1, peer_id1) = create_mock_peer(config.blacklist_timeout, false);
    peer1.expect_is_blocked().times(..2).return_const(true);

    // Create a mock peer - not blocked
    let (mut peer2, peer_id2) = create_mock_peer(config.blacklist_timeout, false);
    peer2.expect_is_blocked().times(..2).return_const(false);

    // Add the mock peers to the peer manager
    peer_manager.add_peer(peer1);
    peer_manager.add_peer(peer2);

    // call handle_established_inbound_connection with the blocked peer
    let res = peer_manager.handle_established_inbound_connection(
        libp2p::swarm::ConnectionId::new_unchecked(0),
        peer_id1,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    // ConnectionHandler doesn't implement Debug so we have to assert the result like that.
    assert!(res.is_err());

    // call handle_established_inbound_connection with the blocked peer
    let res = peer_manager.handle_established_inbound_connection(
        libp2p::swarm::ConnectionId::new_unchecked(0),
        peer_id2,
        &Multiaddr::empty(),
        &Multiaddr::empty(),
    );
    // ConnectionHandler doesn't implement Debug so we have to assert the result like that.
    assert!(res.is_ok());
}
