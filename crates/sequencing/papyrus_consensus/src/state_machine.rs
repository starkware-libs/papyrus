#[cfg(test)]
#[path = "state_machine_test.rs"]
mod state_machine_test;

use std::collections::{HashMap, VecDeque};

use starknet_api::block::BlockHash;

pub type Round = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum StateMachineEvent {
    StartRound(Option<BlockHash>, Round),
    // BlockHash, Round
    Proposal(BlockHash, Round),
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
    quorum: u32,
    proposals: HashMap<Round, BlockHash>,
    // {round: {block_hash: vote_count}
    prevotes: HashMap<Round, HashMap<BlockHash, u32>>,
    precommits: HashMap<Round, HashMap<BlockHash, u32>>,
    // When true, the state machine will wait for a GetProposal event, cacheing all other input
    // events in `events_queue`.
    starting_round: bool,
    events_queue: VecDeque<StateMachineEvent>,
}

impl StateMachine {
    /// total_weight - the total voting weight of all validators for this height.
    pub fn new(total_weight: u32) -> Self {
        Self {
            round: 0,
            step: Step::Propose,
            quorum: (2 * total_weight / 3) + 1,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            starting_round: false,
            events_queue: VecDeque::new(),
        }
    }

    pub fn start(&mut self) -> VecDeque<StateMachineEvent> {
        self.starting_round = true;
        VecDeque::from([StateMachineEvent::StartRound(None, self.round)])
    }

    /// Process the incoming event.
    ///
    /// If we have just started a new round and are awaiting `StartRound` then any event other than
    /// `StartRound` will be cached for processing after `StartRound` is received. If an event we
    /// can handle is received, then we handle it and all other cached events.
    ///
    /// This returns a set of events back to the caller. The caller should not pass the output
    /// events back to the state machine, as it handles these events before returning;
    /// effectively sending the events to itself.
    // This means that the StateMachine handles events the same regardless of whether it was sent by
    // self or a peer. This is in line with the Algorithm 1 in
    // [paper](https://arxiv.org/pdf/1807.04938) and keeps the code simpler.
    pub fn handle_event(&mut self, event: StateMachineEvent) -> VecDeque<StateMachineEvent> {
        // Mimic LOC 18 in the paper; the state machine doesn't
        // handle any events until `getValue` completes.
        if self.starting_round {
            match event {
                StateMachineEvent::StartRound(_, round) if round == self.round => {
                    self.events_queue.push_front(event); // Validator.
                }
                StateMachineEvent::Proposal(_, round) if round == self.round => {
                    self.events_queue.push_front(event); // Proposer.
                }
                _ => {
                    self.events_queue.push_back(event);
                    return VecDeque::new();
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
    ) -> VecDeque<StateMachineEvent> {
        let mut output_events = VecDeque::new();
        while let Some(event) = events_queue.pop_front() {
            // Handle a specific event and then decide which of the output events should also be
            // sent to self.
            for e in self.handle_event_internal(event) {
                match e {
                    StateMachineEvent::Proposal(_, _)
                    | StateMachineEvent::Prevote(_, _)
                    | StateMachineEvent::Precommit(_, _) => {
                        events_queue.push_back(e.clone());
                    }
                    _ => {}
                }
                output_events.push_back(e);
            }
        }
        output_events
    }

    fn handle_event_internal(&mut self, event: StateMachineEvent) -> VecDeque<StateMachineEvent> {
        match event {
            StateMachineEvent::StartRound(block_hash, round) => {
                self.handle_start_round(block_hash, round)
            }
            StateMachineEvent::Proposal(block_hash, round) => {
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

    // Returned to the node if we are not the proposer.
    fn handle_start_round(
        &mut self,
        block_hash: Option<BlockHash>,
        round: u32,
    ) -> VecDeque<StateMachineEvent> {
        assert!(block_hash.is_none(), "Should receive a Proposal instead");
        // Must be true since expect an instant response if we are not the Proposer.
        assert_eq!(self.round, round);
        assert!(self.starting_round);
        self.starting_round = false;
        VecDeque::new()
    }

    // A proposal from a peer (or self) node.
    fn handle_proposal(
        &mut self,
        block_hash: BlockHash,
        round: u32,
    ) -> VecDeque<StateMachineEvent> {
        if round == self.round {
            self.starting_round = false;
        }

        let old = self.proposals.insert(round, block_hash);
        assert!(old.is_none(), "SHC should handle conflicts & replays");
        let mut output = VecDeque::from([StateMachineEvent::Prevote(block_hash, round)]);
        output.append(&mut self.advance_to_step(Step::Prevote));
        output
    }

    // A prevote from a peer (or self) node.
    fn handle_prevote(&mut self, block_hash: BlockHash, round: u32) -> VecDeque<StateMachineEvent> {
        assert_eq!(round, 0, "Only round 0 is supported in this milestone.");
        let prevote_count = self.prevotes.entry(round).or_default().entry(block_hash).or_insert(0);
        *prevote_count += 1;
        if *prevote_count < self.quorum {
            return VecDeque::new();
        }
        self.send_precommit(block_hash, round)
    }

    // A precommit from a peer (or self) node.
    fn handle_precommit(
        &mut self,
        block_hash: BlockHash,
        round: u32,
    ) -> VecDeque<StateMachineEvent> {
        assert_eq!(round, 0, "Only round 0 is supported in this milestone.");
        let precommit_count =
            self.precommits.entry(round).or_default().entry(block_hash).or_insert(0);
        *precommit_count += 1;
        if *precommit_count < self.quorum {
            return VecDeque::new();
        }
        VecDeque::from([StateMachineEvent::Decision(block_hash, round)])
    }

    fn advance_to_step(&mut self, step: Step) -> VecDeque<StateMachineEvent> {
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

    fn check_prevote_quorum(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        let Some((block_hash, count)) = leading_vote(&self.prevotes, round) else {
            return VecDeque::new();
        };
        if *count < self.quorum {
            return VecDeque::new();
        }
        self.send_precommit(*block_hash, round)
    }

    fn check_precommit_quorum(&mut self, round: u32) -> VecDeque<StateMachineEvent> {
        let Some((block_hash, count)) = leading_vote(&self.precommits, round) else {
            return VecDeque::new();
        };
        if *count < self.quorum {
            return VecDeque::new();
        }
        VecDeque::from([StateMachineEvent::Decision(*block_hash, round)])
    }

    fn send_precommit(&mut self, block_hash: BlockHash, round: u32) -> VecDeque<StateMachineEvent> {
        let mut output = VecDeque::from([StateMachineEvent::Precommit(block_hash, round)]);
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
