use std::convert::{TryFrom, TryInto};

use prost::Message;
use starknet_api::block::BlockHash;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::Transaction;

use crate::consensus::{ConsensusMessage, Proposal};
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

impl TryFrom<protobuf::ConsensusMessage> for ConsensusMessage {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::ConsensusMessage) -> Result<Self, Self::Error> {
        use protobuf::consensus_message::Message;

        let Some(message) = value.message else {
            return Err(ProtobufConversionError::MissingField { field_description: "message" });
        };

        match message {
            Message::Proposal(proposal) => Ok(ConsensusMessage::Proposal(proposal.try_into()?)),
        }
    }
}

impl From<ConsensusMessage> for protobuf::ConsensusMessage {
    fn from(value: ConsensusMessage) -> Self {
        match value {
            ConsensusMessage::Proposal(proposal) => protobuf::ConsensusMessage {
                message: Some(protobuf::consensus_message::Message::Proposal(proposal.into())),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ConsensusMessage, protobuf::ConsensusMessage);
