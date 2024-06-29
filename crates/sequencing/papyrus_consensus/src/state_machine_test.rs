use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;

use crate::state_machine::{StateMachine, StateMachineEvent};

#[test]
fn in_order() {
    let mut sm = StateMachine::new(4);

    let mut events = sm.start();
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::StartRound(None, 0));

    // This mimics:
    // Proposer - the SHC building the proposal and passing it back to SM.
    // Validator - receiving a proposal from a peer.
    events = sm.handle_event(StateMachineEvent::Proposal(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Decision(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}

#[test]
fn validator_receives_votes_first() {
    let mut sm = StateMachine::new(4);

    let mut events = sm.start();
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());

    // Send votes first.
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
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
fn cache_events_during_get_proposal() {
    let mut sm = StateMachine::new(4);
    let mut events = sm.start();
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());

    // TODO(matan): When we support NIL votes, we should send them. Real votes without the proposal
    // doesn't make sense.
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    assert!(events.is_empty());

    // Node finishes building the proposal.
    events = sm.handle_event(StateMachineEvent::Proposal(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.pop_front().unwrap(), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}
