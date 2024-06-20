use std::convert::{TryFrom, TryInto};

use prost::Message;
use starknet_api::block::BlockHash;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::Transaction;

use crate::consensus::{ConsensusMessage, Proposal, Vote, VoteType};
use crate::converters::ProtobufConversionError;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::Proposal> for Proposal {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Proposal) -> Result<Self, Self::Error> {
        let transactions = value
            .transactions
            .into_iter()
            .map(|tx| tx.try_into())
            .collect::<Result<Vec<Transaction>, ProtobufConversionError>>()?;

        let height = value.height;
        let contract_address = value
            .proposer
            .ok_or(ProtobufConversionError::MissingField { field_description: "proposer" })?
            .try_into()?;
        let block_hash: StarkHash = value
            .block_hash
            .ok_or(ProtobufConversionError::MissingField { field_description: "block_hash" })?
            .try_into()?;
        let block_hash = BlockHash(block_hash);

        Ok(Proposal { height, proposer: contract_address, transactions, block_hash })
    }
}

impl From<Proposal> for protobuf::Proposal {
    fn from(value: Proposal) -> Self {
        let transactions = value.transactions.into_iter().map(Into::into).collect();

        protobuf::Proposal {
            height: value.height,
            proposer: Some(value.proposer.into()),
            transactions,
            block_hash: Some(value.block_hash.0.into()),
        }
    }
}

impl TryFrom<protobuf::vote::VoteType> for VoteType {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::vote::VoteType) -> Result<Self, Self::Error> {
        match value {
            protobuf::vote::VoteType::Prevote => Ok(VoteType::Prevote),
            protobuf::vote::VoteType::Precommit => Ok(VoteType::Precommit),
        }
    }
}

impl From<VoteType> for protobuf::vote::VoteType {
    fn from(value: VoteType) -> Self {
        match value {
            VoteType::Prevote => protobuf::vote::VoteType::Prevote,
            VoteType::Precommit => protobuf::vote::VoteType::Precommit,
        }
    }
}

impl TryFrom<protobuf::Vote> for Vote {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Vote) -> Result<Self, Self::Error> {
        let vote_type = protobuf::vote::VoteType::try_from(value.vote_type)?.try_into()?;

        let height = value.height;
        let block_hash: StarkHash = value
            .block_hash
            .ok_or(ProtobufConversionError::MissingField { field_description: "block_hash" })?
            .try_into()?;
        let block_hash = BlockHash(block_hash);
        let sender = value
            .voter
            .ok_or(ProtobufConversionError::MissingField { field_description: "sender" })?
            .try_into()?;

        Ok(Vote { vote_type, height, block_hash, voter: sender })
    }
}

impl From<Vote> for protobuf::Vote {
    fn from(value: Vote) -> Self {
        let vote_type = match value.vote_type {
            VoteType::Prevote => protobuf::vote::VoteType::Prevote,
            VoteType::Precommit => protobuf::vote::VoteType::Precommit,
        };

        protobuf::Vote {
            vote_type: vote_type as i32,
            height: value.height,
            block_hash: Some(value.block_hash.0.into()),
            voter: Some(value.voter.into()),
        }
    }
}

impl TryFrom<protobuf::ConsensusMessage> for ConsensusMessage {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::ConsensusMessage) -> Result<Self, Self::Error> {
        use protobuf::consensus_message::Message;

        let Some(message) = value.message else {
            return Err(ProtobufConversionError::MissingField { field_description: "message" });
        };

        match message {
            Message::Proposal(proposal) => Ok(ConsensusMessage::Proposal(proposal.try_into()?)),
            Message::Vote(vote) => Ok(ConsensusMessage::Vote(vote.try_into()?)),
        }
    }
}

impl From<ConsensusMessage> for protobuf::ConsensusMessage {
    fn from(value: ConsensusMessage) -> Self {
        match value {
            ConsensusMessage::Proposal(proposal) => protobuf::ConsensusMessage {
                message: Some(protobuf::consensus_message::Message::Proposal(proposal.into())),
            },
            ConsensusMessage::Vote(vote) => protobuf::ConsensusMessage {
                message: Some(protobuf::consensus_message::Message::Vote(vote.into())),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ConsensusMessage, protobuf::ConsensusMessage);
