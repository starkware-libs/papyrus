#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

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
pub struct StateMachine {
    round: Round,
    step: Step,
    id: ValidatorId,
    quorum: u32,
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
    /// total_weight - the total voting weight of all validators for this height.
    pub fn new(
        id: ValidatorId,
        total_weight: u32,
        leader_fn: Box<dyn Fn(Round) -> ValidatorId>,
    ) -> Self {
        Self {
            round: 0,
            step: Step::Propose,
            id,
            quorum: (2 * total_weight / 3) + 1,
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

    /// Process the incoming event.
    ///
    /// If we have just started a new round and are awaiting `GetProposal` then any event other than
    /// `GetProposal` will be cached for processing after `GetProposal` is received. If an event we
    /// can handle is received, then we handle it and all other cached events.
    ///
    /// This returns a set of events back to the caller. The caller should not pass the output
    /// events back to the state machine, as it handles these events before returning;
    /// effectively sending the events to itself.
    // This means that the StateMachine handles events the same regardless of whether it was sent by
    // self or a peer. This is in line with the Algorithm 1 in
    // [paper](https://arxiv.org/pdf/1807.04938) and keeps the code simpler.
    pub fn handle_event(&mut self, event: StateMachineEvent) -> Vec<StateMachineEvent> {
        // Mimic LOC 18 in the paper; the state machine doesn't
        // handle any events until `getValue` completes.
        if self.awaiting_get_proposal {
            match event {
                StateMachineEvent::GetProposal(_, round) if round == self.round => {
                    // `awaiting_get_proposal` is reset when handling `GetProposal` this guarantees
                    // that only the relevant `GetProposal` is handled, as opposed to potentially
                    // others from earlier rounds.
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
        while let Some(event) = events_queue.pop_front() {
            // Handle a specific event and then decide which of the output events should also be
            // sent to self.
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
            // TODO(matan): Consider if this should be an assertion or return an error?
            return Vec::new();
        }
        self.awaiting_get_proposal = false;
        let block_hash = block_hash.expect("GetProposal event must have a block_hash");
        let mut output = vec![StateMachineEvent::Propose(block_hash, round)];
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // A proposal from a peer (or self) node.
    fn handle_proposal(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        let old = self.proposals.insert(round, block_hash);
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        let mut output = vec![StateMachineEvent::Prevote(block_hash, round)];
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // A prevote from a peer (or self) node.
    fn handle_prevote(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        assert_eq!(round, 0, "Only round 0 is supported in this milestone.");
        let prevote_count = self.prevotes.entry(round).or_default().entry(block_hash).or_insert(0);
        *prevote_count += 1;
        if *prevote_count < self.quorum {
            return Vec::new();
        }
        self.send_precommit(block_hash, round)
    }

    // A precommit from a peer (or self) node.
    fn handle_precommit(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        assert_eq!(round, 0, "Only round 0 is supported in this milestone.");
        let precommit_count =
            self.precommits.entry(round).or_default().entry(block_hash).or_insert(0);
        *precommit_count += 1;
        if *precommit_count < self.quorum {
            return Vec::new();
        }
        vec![StateMachineEvent::Decision(block_hash, round)]
    }

    fn advance_to_step(&mut self, step: Step) -> Vec<StateMachineEvent> {
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
        if *count < self.quorum {
            return Vec::new();
        }
        self.send_precommit(*block_hash, round)
    }

    fn check_precommit_quorum(&mut self, round: u32) -> Vec<StateMachineEvent> {
        let Some((block_hash, count)) = leading_vote(&self.precommits, round) else {
            return Vec::new();
        };
        if *count < self.quorum {
            return Vec::new();
        }
        vec![StateMachineEvent::Decision(*block_hash, round)]
    }

    fn send_precommit(&mut self, block_hash: BlockHash, round: u32) -> Vec<StateMachineEvent> {
        let mut output = vec![StateMachineEvent::Precommit(block_hash, round)];
        output.append(&mut self.advance_to_step(Step::Precommit));
        output
    }
}

fn leading_vote(
    votes: &HashMap<u32, HashMap<BlockHash, u32>>,
    round: u32,
) -> Option<(&BlockHash, &u32)> {
    // We don't care which value is chosen in the case of a tie, since consensus requires 2/3+1.
    votes.get(&round)?.iter().max_by(|a, b| a.1.cmp(b.1))
}
