#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

use std::collections::{HashMap, VecDeque};
use std::vec;

use starknet_api::block::BlockHash;

use crate::types::ValidatorId;

#[derive(Debug, Clone, PartialEq)]
pub enum StateMachineEvent {
    // BlockHash, Round
    GetProposal(Option<BlockHash>, u32),
    // BlockHash, Round, ValidRound
    Propose(BlockHash, u32),
    Prevote(BlockHash, u32),
    Precommit(BlockHash, u32),
    // SingleHeightConsensus can figure out the relevant precommits, as the StateMachine only
    // records aggregated votes.
    Decision(BlockHash, u32),
}

pub enum Step {
    Propose,
    Prevote,
    Precommit,
}

/// State Machine for Milestone 2. Major assumptions:
/// 1. SHC handles replays and conflicts.
/// 2. SM must handle "out of order" messages (E.g. vote arrives before proposal).
/// 3. Only valid proposals (e.g. no NIL)
/// 4. No network failures - together with 3 this means we only support round 0.
pub struct StateMachine {
    round: u32,
    step: Step,
    id: ValidatorId,
    validators: Vec<ValidatorId>,
    proposals: HashMap<u32, BlockHash>,
    // {round: {block_hash: vote_count}
    prevotes: HashMap<u32, HashMap<BlockHash, u32>>,
    precommits: HashMap<u32, HashMap<BlockHash, u32>>,
    // When true, the state machine will wait for a GetProposal event, cacheing all other input
    // events in `events_queue`.
    awaiting_get_proposal: bool,
    events_queue: VecDeque<StateMachineEvent>,
    leader_fn: Box<dyn Fn(u32) -> ValidatorId>,
}

impl StateMachine {
    pub fn new(
        id: ValidatorId,
        validators: Vec<ValidatorId>,
        leader_fn: Box<dyn Fn(u32) -> ValidatorId>,
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
        // Mimic LOC 18 in the paper; the state machine doesn't handle any events until `getValue`
        // completes.
        if self.awaiting_get_proposal {
            match event {
                StateMachineEvent::GetProposal(_, round) if round == self.round => {
                    self.events_queue.push_front(event);
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
        while !events_queue.is_empty() {
            let event = events_queue.pop_front().unwrap();
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

    fn handle_event_internal(&mut self, event: StateMachineEvent) -> Vec<StateMachineEvent> {
        match event {
            StateMachineEvent::GetProposal(block_hash, round) => {
                self.handle_get_proposal(block_hash, round)
            }
            StateMachineEvent::Propose(block_hash, round) => {
                self.handle_proposal(block_hash, round)
            }
            StateMachineEvent::Prevote(block_hash, round) => self.handle_prevote(block_hash, round),
            StateMachineEvent::Precommit(block_hash, round) => {
                self.handle_precommit(block_hash, round)
            }
            StateMachineEvent::Decision(_, _) => {
                unimplemented!(
                    "If the caller knows of a decision, it can just drop the state machine."
                )
            }
        }
    }

    // The node finishes building a proposal in response to the state machine sending out
    // GetProposal.
    fn handle_get_proposal(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
    ) -> Vec<StateMachineEvent> {
        if !self.awaiting_get_proposal || self.round != round {
            return Vec::new();
        }
        self.awaiting_get_proposal = false;
        let block_hash = block_hash.expect("GetProposal event must have a block_hash");
        let mut output = vec![StateMachineEvent::Propose(block_hash, round)];
        output.append(&mut self.advance_step(Step::Prevote));
        output
    }

    // A proposal from a peer (or self) node.
    fn handle_proposal(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        let old = self.proposals.insert(round, block_hash);
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        let mut output = vec![StateMachineEvent::Prevote(block_hash, round)];
        output.append(&mut self.advance_step(Step::Prevote));
        output
    }

    fn handle_prevote(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        assert_eq!(round, 0, "Only round 0 is supported in this milestone.");
        let prevote_count = self.prevotes.entry(round).or_default().entry(block_hash).or_insert(0);
        *prevote_count += 1;
        if *prevote_count < self.quorum() {
            return Vec::new();
        }
        self.send_precommit(block_hash, round)
    }

    fn handle_precommit(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        assert_eq!(round, 0, "Only round 0 is supported in this milestone.");
        let precommit_count =
            self.precommits.entry(round).or_default().entry(block_hash).or_insert(0);
        *precommit_count += 1;
        if *precommit_count < self.quorum() {
            return Vec::new();
        }
        vec![StateMachineEvent::Decision(block_hash, round)]
    }

    fn advance_step(&mut self, step: Step) -> Vec<StateMachineEvent> {
        self.step = step;
        // Check for an existing quorum in case messages arrived out of order.
        match self.step {
            Step::Propose => {
                unimplemented!("Handled by `advance_round`")
            }
            Step::Prevote => self.check_prevote_quorum(self.round),
            Step::Precommit => self.check_precommit_quorum(self.round),
        }
    }

    fn check_prevote_quorum(&mut self, round: u32) -> Vec<StateMachineEvent> {
        let Some((block_hash, count)) = leading_vote(&self.prevotes, round) else {
            return Vec::new();
        };
        if *count < self.quorum() {
            return Vec::new();
        }
        self.send_precommit(*block_hash, round)
    }

    fn check_precommit_quorum(&mut self, round: u32) -> Vec<StateMachineEvent> {
        let Some((block_hash, count)) = leading_vote(&self.precommits, round) else {
            return Vec::new();
        };
        if *count < self.quorum() {
            return Vec::new();
        }
        vec![StateMachineEvent::Decision(*block_hash, round)]
    }

    fn send_precommit(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        let mut output = vec![StateMachineEvent::Precommit(block_hash, round)];
        output.append(&mut self.advance_step(Step::Precommit));
        output
    }

    fn quorum(&self) -> u32 {
        let q = (2 * self.validators.len() / 3) + 1;
        q as u32
    }
}

fn leading_vote(
    votes: &HashMap<u32, HashMap<BlockHash, u32>>,
    round: u32,
) -> Option<(&BlockHash, &u32)> {
    // We don't care which value is chosen in the case of a tie, since consensus requires 2/3+1.
    votes.get(&round)?.iter().max_by(|a, b| a.1.cmp(b.1))
}
