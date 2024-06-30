use starknet_api::block::BlockHash;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::ValidatorId;

fn proposer() -> ValidatorId {
    ContractAddress::try_from(Felt::ONE).unwrap()
}

fn validator() -> ValidatorId {
    ContractAddress::try_from(Felt::TWO).unwrap()
}

fn create_state_machine(is_proposer: bool) -> StateMachine {
    StateMachine::new(
        if is_proposer { proposer() } else { validator() },
        4,
        Box::new(|_| proposer()),
    )
}

#[test_case(true; "proposer")]
#[test_case(false; "validator")]
fn in_order(is_proposer: bool) {
    let mut sm = create_state_machine(is_proposer);

    let mut events = sm.start();
    if is_proposer {
        assert_eq!(events.remove(0), StateMachineEvent::GetProposal(None, 0));
        assert!(events.is_empty());

        events = sm.handle_event(StateMachineEvent::GetProposal(Some(BlockHash(Felt::ONE)), 0));
        assert_eq!(events.remove(0), StateMachineEvent::Propose(BlockHash(Felt::ONE), 0));
    } else {
        assert!(events.is_empty());
        events = sm.handle_event(StateMachineEvent::Propose(BlockHash(Felt::ONE), 0));
    }
    assert_eq!(events.remove(0), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());

    events = sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Decision(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}

#[test]
fn validator_receives_votes_first() {
    let mut sm = create_state_machine(false);

    let mut events = sm.start();
    assert!(events.is_empty());

    // Send votes first.
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0)));
    assert!(events.is_empty());

    // Finally the proposal arrives.
    events = sm.handle_event(StateMachineEvent::Propose(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Decision(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}

#[test]
fn cache_events_during_get_proposal() {
    let mut sm = create_state_machine(true);
    let mut events = sm.start();
    assert_eq!(events.remove(0), StateMachineEvent::GetProposal(None, 0));
    assert!(events.is_empty());

    // TODO(matan): When we support NIL votes, we should send them. Real votes without the proposal
    // doesn't make sense.
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    events.append(&mut sm.handle_event(StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0)));
    assert!(events.is_empty());

    // Node finishes building the proposal.
    events = sm.handle_event(StateMachineEvent::GetProposal(Some(BlockHash(Felt::ONE)), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Propose(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Prevote(BlockHash(Felt::ONE), 0));
    assert_eq!(events.remove(0), StateMachineEvent::Precommit(BlockHash(Felt::ONE), 0));
    assert!(events.is_empty());
}
