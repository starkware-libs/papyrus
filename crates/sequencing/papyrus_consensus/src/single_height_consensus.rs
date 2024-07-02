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

/// Struct which represents a single height of consensus. Each height is expected to be begun with a
/// call to `start`, which is relevant if we are the proposer for this height's first round. SHC
/// receives messages directly as parameters to function calls. It can send out messages "directly"
/// to the network, and returning a decision to the caller.
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
        skip(self, init, content_receiver, fin_receiver),
        fields(height = %self.height),
        level = "debug",
    )]
    pub(crate) async fn handle_proposal(
        &mut self,
        init: ProposalInit,
        content_receiver: mpsc::Receiver<<BlockT as ConsensusBlock>::ProposalChunk>,
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

        let block_receiver = self.context.validate_proposal(self.height, content_receiver).await;
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
        let sm_proposal = StateMachineEvent::Proposal(block.id(), 0);
        // TODO(matan): Handle multiple rounds.
        self.proposals.insert(0, block);
        let sm_events = self.state_machine.handle_event(sm_proposal);
        self.handle_state_machine_events(sm_events).await
    }

    /// Handle messages from peer nodes.
    #[instrument(skip(self), level = "debug")]
    pub(crate) async fn handle_message(
        &mut self,
        message: ConsensusMessage,
    ) -> Result<Option<BlockT>, ConsensusError> {
        match message {
            ConsensusMessage::Proposal(_) => {
                unimplemented!("Proposals should use `handle_proposal` due to fake streaming")
            }
            ConsensusMessage::Vote(vote) => match vote.vote_type {
                papyrus_protobuf::consensus::VoteType::Prevote => self.handle_prevote(vote).await,
                papyrus_protobuf::consensus::VoteType::Precommit => {
                    self.handle_precommit(vote).await
                }
            },
        }
    }

    async fn handle_prevote(&mut self, vote: Vote) -> Result<Option<BlockT>, ConsensusError> {
        if let Some(old) = self.prevotes.get(&(0, vote.voter)) {
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
        let sm_prevote = StateMachineEvent::Prevote(vote.block_hash, 0);
        self.prevotes.insert((0, vote.voter), vote);
        let sm_events = self.state_machine.handle_event(sm_prevote);
        self.handle_state_machine_events(sm_events).await
    }

    async fn handle_precommit(&mut self, vote: Vote) -> Result<Option<BlockT>, ConsensusError> {
        if let Some(old) = self.precommits.get(&(0, vote.voter)) {
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
        let sm_precommit = StateMachineEvent::Precommit(vote.block_hash, 0);
        self.precommits.insert((0, vote.voter), vote);
        let sm_events = self.state_machine.handle_event(sm_precommit);
        self.handle_state_machine_events(sm_events).await
    }

    // Handle events output by the state machine.
    async fn handle_state_machine_events(
        &mut self,
        mut events: VecDeque<StateMachineEvent>,
    ) -> Result<Option<BlockT>, ConsensusError> {
        while let Some(event) = events.pop_front() {
            let res = match event {
                StateMachineEvent::StartRound(block_hash, round) => {
                    events.append(&mut self.handle_sm_start_round(block_hash, round).await);
                    Ok(None)
                }
                StateMachineEvent::Proposal(_, _) => {
                    // Ignore proposals sent by the StateMachine as SHC already sent this out when
                    // responding to a StartRound.
                    Ok(None)
                }
                StateMachineEvent::Decision(block_hash, round) => {
                    let block = self.proposals.remove(&round).expect("Block not found.");
                    assert_eq!(
                        block.id(),
                        block_hash,
                        "StateMachine block hash should match the stored block"
                    );
                    return Ok(Some(block));
                }
                StateMachineEvent::Prevote(block_hash, round) => {
                    self.handle_sm_prevote(block_hash, round).await
                }
                StateMachineEvent::Precommit(block_hash, round) => {
                    self.handle_sm_precommit(block_hash, round).await
                }
            };
            if let Some(block) = res? {
                return Ok(Some(block));
            }
        }
        Ok(None)
    }

    #[instrument(skip(self), level = "debug")]
    async fn handle_sm_start_round(
        &mut self,
        block_hash: Option<BlockHash>,
        round: Round,
    ) -> VecDeque<StateMachineEvent> {
        assert!(block_hash.is_none(), "Reproposing is not yet supported");
        let proposer_id = self.context.proposer(&self.validators, self.height);
        if proposer_id != self.id {
            debug!("Validator");
            return self.state_machine.handle_event(StateMachineEvent::StartRound(None, round));
        }
        debug!("Proposer");

        let (content_receiver, block_receiver) = self.context.build_proposal(self.height).await;
        let (fin_sender, fin_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, proposer: self.id };
        // Peering is a permanent component, so if sending to it fails we cannot continue.
        self.context
            .propose(init, content_receiver, fin_receiver)
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

    async fn handle_sm_prevote(
        &mut self,
        block_hash: BlockHash,
        round: Round,
    ) -> Result<Option<BlockT>, ConsensusError> {
        let vote = Vote {
            vote_type: VoteType::Prevote,
            height: self.height.0,
            block_hash,
            voter: self.id,
        };
        let old = self.prevotes.insert((round, self.id), vote.clone());
        assert!(
            old.is_none(),
            "The state machine should not send repeat votes: old={:?} new={:?}",
            old,
            vote
        );
        self.context.broadcast(ConsensusMessage::Vote(vote)).await?;
        Ok(None)
    }

    async fn handle_sm_precommit(
        &mut self,
        block_hash: BlockHash,
        round: Round,
    ) -> Result<Option<BlockT>, ConsensusError> {
        let vote = Vote {
            vote_type: VoteType::Precommit,
            height: self.height.0,
            block_hash,
            voter: self.id,
        };
        let old = self.precommits.insert((round, self.id), vote.clone());
        assert!(
            old.is_none(),
            "The state machine should not send repeat votes: old={:?} new={:?}",
            old,
            vote
        );
        self.context.broadcast(ConsensusMessage::Vote(vote)).await?;
        Ok(None)
    }
}
