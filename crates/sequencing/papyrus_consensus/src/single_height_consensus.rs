#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use futures::channel::{mpsc, oneshot};
use papyrus_protobuf::consensus::{ConsensusMessage, Vote, VoteType};
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument};

use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    ProposalInit,
    Round,
    ValidatorId,
};

const ROUND_ZERO: Round = 0;

/// Struct which represents a single height of consensus. Each height is expected to be begun with a
/// call to `start`, which is relevant if we are the proposer for this height's first round.
/// SingleHeightConsensus receives messages directly as parameters to function calls. It can send
/// out messages "directly" to the network, and returning a decision to the caller.
pub(crate) struct SingleHeightConsensus<BlockT>
where
    BlockT: ConsensusBlock,
{
    height: BlockNumber,
    context: Arc<dyn ConsensusContext<Block = BlockT>>,
    validators: Vec<ValidatorId>,
    id: ValidatorId,
    state_machine: StateMachine,
    proposals: HashMap<Round, BlockT>,
    prevotes: HashMap<(Round, ValidatorId), Vote>,
    precommits: HashMap<(Round, ValidatorId), Vote>,
}

impl<BlockT> SingleHeightConsensus<BlockT>
where
    BlockT: ConsensusBlock,
{
    pub(crate) async fn new(
        height: BlockNumber,
        context: Arc<dyn ConsensusContext<Block = BlockT>>,
        id: ValidatorId,
    ) -> Self {
        let validators = context.validators(height).await;
        // TODO(matan): Use actual weights, not just `len`.
        let state_machine = StateMachine::new(validators.len() as u32);
        Self {
            height,
            context,
            validators,
            id,
            state_machine,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
        }
    }

    #[instrument(skip(self), fields(height=self.height.0), level = "debug")]
    pub(crate) async fn start(&mut self) -> Result<Option<BlockT>, ConsensusError> {
        info!("Starting consensus with validators {:?}", self.validators);
        let events = self.state_machine.start();
        self.handle_state_machine_events(events).await
    }

    /// Receive a proposal from a peer node. Returns only once the proposal has been fully received
    /// and processed.
    #[instrument(
        skip(self, init, p2p_messages_receiver, fin_receiver),
        fields(height = %self.height),
        level = "debug",
    )]
    pub(crate) async fn handle_proposal(
        &mut self,
        init: ProposalInit,
        p2p_messages_receiver: mpsc::Receiver<<BlockT as ConsensusBlock>::ProposalChunk>,
        fin_receiver: oneshot::Receiver<BlockHash>,
    ) -> Result<Option<BlockT>, ConsensusError> {
        debug!(
            "Received proposal: proposal_height={}, proposer={:?}",
            init.height.0, init.proposer
        );
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if init.height != self.height {
            let msg = format!("invalid height: expected {:?}, got {:?}", self.height, init.height);
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height, msg));
        }
        if init.proposer != proposer_id {
            let msg =
                format!("invalid proposer: expected {:?}, got {:?}", proposer_id, init.proposer);
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height, msg));
        }

        let block_receiver =
            self.context.validate_proposal(self.height, p2p_messages_receiver).await;
        // TODO(matan): Actual Tendermint should handle invalid proposals.
        let block = block_receiver.await.map_err(|_| {
            ConsensusError::InvalidProposal(
                proposer_id,
                self.height,
                "block validation failed".into(),
            )
        })?;
        // TODO(matan): Actual Tendermint should handle invalid proposals.
        let fin = fin_receiver.await.map_err(|_| {
            ConsensusError::InvalidProposal(
                proposer_id,
                self.height,
                "proposal fin never received".into(),
            )
        })?;
        // TODO(matan): Switch to signature validation and handle invalid proposals.
        if block.id() != fin {
            return Err(ConsensusError::InvalidProposal(
                proposer_id,
                self.height,
                "block signature doesn't match expected block hash".into(),
            ));
        }
        let sm_proposal = StateMachineEvent::Proposal(block.id(), ROUND_ZERO);
        // TODO(matan): Handle multiple rounds.
        self.proposals.insert(ROUND_ZERO, block);
        let sm_events = self.state_machine.handle_event(sm_proposal);
        self.handle_state_machine_events(sm_events).await
    }

    /// Handle messages from peer nodes.
    pub(crate) async fn handle_message(
        &mut self,
        message: ConsensusMessage,
    ) -> Result<Option<BlockT>, ConsensusError> {
        debug!("Received message: {:?}", message);
        match message {
            ConsensusMessage::Proposal(_) => {
                unimplemented!("Proposals should use `handle_proposal` due to fake streaming")
            }
            ConsensusMessage::Vote(vote) => self.handle_vote(vote).await,
        }
    }

    async fn handle_vote(&mut self, vote: Vote) -> Result<Option<BlockT>, ConsensusError> {
        let (votes, sm_vote) = match vote.vote_type {
            VoteType::Prevote => {
                (&mut self.prevotes, StateMachineEvent::Prevote(vote.block_hash, ROUND_ZERO))
            }
            VoteType::Precommit => {
                (&mut self.precommits, StateMachineEvent::Precommit(vote.block_hash, ROUND_ZERO))
            }
        };
        if let Some(old) = votes.get(&(ROUND_ZERO, vote.voter)) {
            if old.block_hash != vote.block_hash {
                return Err(ConsensusError::Equivocation(
                    self.height,
                    ConsensusMessage::Vote(old.clone()),
                    ConsensusMessage::Vote(vote),
                ));
            } else {
                // Replay, ignore.
                return Ok(None);
            }
        }

        votes.insert((ROUND_ZERO, vote.voter), vote);
        let sm_events = self.state_machine.handle_event(sm_vote);
        self.handle_state_machine_events(sm_events).await
    }

    // Handle events output by the state machine.
    async fn handle_state_machine_events(
        &mut self,
        mut events: VecDeque<StateMachineEvent>,
    ) -> Result<Option<BlockT>, ConsensusError> {
        while let Some(event) = events.pop_front() {
            match event {
                StateMachineEvent::StartRound(block_hash, round) => {
                    events.append(
                        &mut self.handle_state_machine_start_round(block_hash, round).await,
                    );
                }
                StateMachineEvent::Proposal(_, _) => {
                    // Ignore proposals sent by the StateMachine as SingleHeightConsensus already
                    // sent this out when responding to a StartRound.
                }
                StateMachineEvent::Decision(block_hash, round) => {
                    let block = self
                        .proposals
                        .remove(&round)
                        .expect("StateMachine arrived at an unknown decision");
                    assert_eq!(
                        block.id(),
                        block_hash,
                        "StateMachine block hash should match the stored block"
                    );
                    return Ok(Some(block));
                }
                StateMachineEvent::Prevote(block_hash, round) => {
                    self.handle_state_machine_vote(block_hash, round, VoteType::Prevote).await?;
                }
                StateMachineEvent::Precommit(block_hash, round) => {
                    self.handle_state_machine_vote(block_hash, round, VoteType::Precommit).await?;
                }
            }
        }
        Ok(None)
    }

    #[instrument(skip(self), level = "debug")]
    async fn handle_state_machine_start_round(
        &mut self,
        block_hash: Option<BlockHash>,
        round: Round,
    ) -> VecDeque<StateMachineEvent> {
        // TODO(matan): Support re-proposing validValue.
        assert!(block_hash.is_none(), "Reproposing is not yet supported");
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if proposer_id != self.id {
            debug!("Validator");
            return self.state_machine.handle_event(StateMachineEvent::StartRound(None, round));
        }
        debug!("Proposer");

        let (p2p_messages_receiver, block_receiver) =
            self.context.build_proposal(self.height).await;
        let (fin_sender, fin_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, proposer: self.id };
        // Peering is a permanent component, so if sending to it fails we cannot continue.
        self.context
            .propose(init, p2p_messages_receiver, fin_receiver)
            .await
            .expect("Failed sending Proposal to Peering");
        let block = block_receiver.await.expect("Block building failed.");
        let id = block.id();
        // If we choose to ignore this error, we should carefully consider how this affects
        // Tendermint. The partially synchronous model assumes all messages arrive at some point,
        // and this failure means this proposal will never arrive.
        //
        // TODO(matan): Switch this to the Proposal signature.
        fin_sender.send(id).expect("Failed to send ProposalFin to Peering.");
        let old = self.proposals.insert(round, block);
        assert!(old.is_none(), "There should be no entry for this round.");

        // TODO(matan): Send to the state machine and handle voting.
        self.state_machine.handle_event(StateMachineEvent::StartRound(Some(id), round))
    }

    async fn handle_state_machine_vote(
        &mut self,
        block_hash: BlockHash,
        round: Round,
        vote_type: VoteType,
    ) -> Result<Option<BlockT>, ConsensusError> {
        let votes = match vote_type {
            VoteType::Prevote => &mut self.prevotes,
            VoteType::Precommit => &mut self.precommits,
        };
        let vote = Vote { vote_type, height: self.height.0, block_hash, voter: self.id };
        if let Some(old) = votes.insert((round, self.id), vote.clone()) {
            // TODO(matan): Consider refactoring not to panic, rather log and return the error.
            panic!("State machine should not send repeat votes: old={:?}, new={:?}", old, vote);
        }
        self.context.broadcast(ConsensusMessage::Vote(vote)).await?;
        Ok(None)
    }
}
