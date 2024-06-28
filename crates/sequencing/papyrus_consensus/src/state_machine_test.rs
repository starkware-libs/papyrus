use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::Round;
use crate::state_machine::{StateMachine, StateMachineEvent};

const BLOCK_HASH: BlockHash = BlockHash(Felt::ONE);
const ROUND: Round = 0;

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn events_arrive_in_ideal_order(is_proposer: bool) {
    let mut state_machine = StateMachine::new(4);

    let mut events = state_machine.start();
    assert_eq!(events.remove(0), StateMachineEvent::StartRound(None, ROUND));
    if is_proposer {
        events = state_machine.handle_event(StateMachineEvent::StartRound(Some(BLOCK_HASH), ROUND));
        assert_eq!(events.remove(0), StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    } else {
        state_machine.handle_event(StateMachineEvent::StartRound(None, ROUND));
        assert!(events.is_empty());
        events = state_machine.handle_event(StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    }
    assert_eq!(events.remove(0), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert!(events.is_empty());

    events = state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert!(events.is_empty());

    events = state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(events.is_empty());

    events = state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(events.is_empty());

    events = state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert_eq!(events.remove(0), StateMachineEvent::Decision(BLOCK_HASH, ROUND));
    assert!(events.is_empty());
}

#[test]
fn validator_receives_votes_first() {
    let mut state_machine = StateMachine::new(4);

    let mut events = state_machine.start();
    assert_eq!(events.remove(0), StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());
    events.append(&mut state_machine.handle_event(StateMachineEvent::StartRound(None, 0)));
    assert!(events.is_empty());

    // Receives votes from all the other nodes first (more than minimum for a quorum).
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Precommit(BLOCK_HASH, ROUND)));
    assert!(events.is_empty());

    // Finally the proposal arrives.
    events = state_machine.handle_event(StateMachineEvent::Proposal(BLOCK_HASH, ROUND));
    assert_eq!(events.remove(0), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert_eq!(events.remove(0), StateMachineEvent::Decision(BLOCK_HASH, ROUND));
    assert!(events.is_empty());
}

#[test]
fn buffer_events_during_start_round() {
    let mut state_machine = StateMachine::new(4);
    let mut events = state_machine.start();
    assert_eq!(events.remove(0), StateMachineEvent::StartRound(None, 0));
    assert!(events.is_empty());

    // TODO(matan): When we support NIL votes, we should send them. Real votes without the proposal
    // doesn't make sense.
    events.append(&mut state_machine.handle_event(StateMachineEvent::Proposal(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    events.append(&mut state_machine.handle_event(StateMachineEvent::Prevote(BLOCK_HASH, ROUND)));
    assert!(events.is_empty());

    // Node finishes building the proposal.
    events = state_machine.handle_event(StateMachineEvent::StartRound(None, 0));
    assert_eq!(events.remove(0), StateMachineEvent::Prevote(BLOCK_HASH, ROUND));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BLOCK_HASH, ROUND));
    assert!(events.is_empty());
}
