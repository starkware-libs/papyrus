use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::state_machine::{StateMachine, StateMachineEvent};

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn in_order(is_proposer: bool) {
    let mut sm = StateMachine::new(4);

    let mut events = sm.start();
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::StartRound(None, 0));
    if is_proposer {
        events = sm.handle_event(StateMachineEvent::StartRound(Some(BlockHash(Felt::ONE)), 0));
        assert_eq!(
            events.pop_front().unwrap(),
            StateMachineEvent::Proposal(BlockHash(Felt::ONE), 0)
        );
    } else {
        sm.handle_event(StateMachineEvent::StartRound(None, 0));
        assert!(events.is_empty());
        events = sm.handle_event(StateMachineEvent::Proposal(BlockHash(Felt::ONE), 0));
    }
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Decision(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}

#[test]
fn validator_receives_votes_first() {
    let mut sm = StateMachine::new(4);

    let mut events = sm.start();
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());
    events = sm.handle_event(StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0)));
    assert!(events.is_empty());

    // Finally the proposal arrives.
    events = sm.handle_event(StateMachineEvent::Proposal(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Decision(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}

#[test]
fn cache_events_during_start_round() {
    let mut sm = StateMachine::new(4);
    let mut events = sm.start();
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());

    // TODO(matan): When we support NIL votes, we should send them. Real votes without the proposal
    // doesn't make sense.
    events.append(&mut sm.handle_event(StateMachineEvent::Proposal(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    assert!(events.is_empty());

    // Node finishes building the proposal.
    events = sm.handle_event(StateMachineEvent::StartRound(None, 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}
