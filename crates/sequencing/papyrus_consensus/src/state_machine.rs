use std::collections::{HashMap, VecDeque};
use std::vec;

use starknet_api::block::BlockHash;

use crate::types::ValidatorId;

pub type Round = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum StateMachineEvent {
    GetProposal(Option<BlockHash>, Round),
    // BlockHash, Round
    Propose(BlockHash, Round),
    Prevote(BlockHash, Round),
    Precommit(BlockHash, Round),
    // SingleHeightConsensus can figure out the relevant precommits, as the StateMachine only
    // records aggregated votes.
    Decision(BlockHash, Round),
}

pub enum Step {
    Propose,
    Prevote,
    Precommit,
}

/// State Machine. Major assumptions:
/// 1. SHC handles replays and conflicts.
/// 2. SM must handle "out of order" messages (E.g. vote arrives before proposal).
/// 3. Only valid proposals (e.g. no NIL)
/// 4. No network failures - together with 3 this means we only support round 0.
#[allow(dead_code)]
pub struct StateMachine {
    round: Round,
    step: Step,
    id: ValidatorId,
    validators: Vec<ValidatorId>,
    proposals: HashMap<Round, BlockHash>,
    // {round: {block_hash: vote_count}
    prevotes: HashMap<Round, HashMap<BlockHash, u32>>,
    precommits: HashMap<Round, HashMap<BlockHash, u32>>,
    // When true, the state machine will wait for a GetProposal event, cacheing all other input
    // events in `events_queue`.
    awaiting_get_proposal: bool,
    events_queue: VecDeque<StateMachineEvent>,
    leader_fn: Box<dyn Fn(Round) -> ValidatorId>,
}

impl StateMachine {
    pub fn new(
        id: ValidatorId,
        validators: Vec<ValidatorId>,
        leader_fn: Box<dyn Fn(Round) -> ValidatorId>,
    ) -> Self {
        Self {
            round: 0,
            step: Step::Propose,
            id,
            validators,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            awaiting_get_proposal: false,
            events_queue: VecDeque::new(),
            leader_fn,
        }
    }

    pub fn start(&mut self) -> Vec<StateMachineEvent> {
        if self.id != (self.leader_fn)(self.round) {
            return Vec::new();
        }
        self.awaiting_get_proposal = true;
        // TODO(matan): Initiate timeout proposal which can lead to round skipping.
        vec![StateMachineEvent::GetProposal(None, self.round)]
    }

    pub fn handle_event(&mut self, event: StateMachineEvent) -> Vec<StateMachineEvent> {
        // Mimic LOC 18 in the [paper](https://arxiv.org/pdf/1807.04938); the state machine doesn't
        // handle any events until `getValue` completes.
        if self.awaiting_get_proposal {
            match event {
                StateMachineEvent::GetProposal(_, round) if round == self.round => {
                    self.events_queue.push_front(event);
                    self.awaiting_get_proposal = false;
                }
                _ => {
                    self.events_queue.push_back(event);
                    return Vec::new();
                }
            }
        } else {
            self.events_queue.push_back(event);
        }

        // The events queue only maintains state while we are waiting for a proposal.
        let events_queue = std::mem::take(&mut self.events_queue);
        self.handle_enqueued_events(events_queue)
    }

    fn handle_enqueued_events(
        &mut self,
        mut events_queue: VecDeque<StateMachineEvent>,
    ) -> Vec<StateMachineEvent> {
        let mut output_events = Vec::new();
        while let Some(event) = events_queue.pop_front() {
            for e in self.handle_event_internal(event) {
                match e {
                    StateMachineEvent::Propose(_, _)
                    | StateMachineEvent::Prevote(_, _)
                    | StateMachineEvent::Precommit(_, _) => {
                        events_queue.push_back(e.clone());
                    }
                    _ => {}
                }
                output_events.push(e);
            }
        }
        output_events
    }

    fn handle_event_internal(&mut self, _event: StateMachineEvent) -> Vec<StateMachineEvent> {
        todo!()
    }
}
