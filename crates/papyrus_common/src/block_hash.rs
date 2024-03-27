#[cfg(test)]
#[path = "block_hash_test.rs"]
mod block_hash_test;

use std::iter::zip;

use starknet_api::block::{BlockBody, BlockHash, BlockHeader};
use starknet_api::core::{ChainId, EventCommitment, TransactionCommitment};
use starknet_api::hash::{pedersen_hash, StarkFelt, StarkHash};
use starknet_api::transaction::{
    DeployAccountTransaction,
    Event,
    Transaction,
    TransactionHash,
    TransactionOutput,
};
use starknet_api::StarknetApiError;

use crate::patricia_hash_tree::calculate_root;
use crate::transaction_hash::{ascii_as_felt, HashChain, ZERO};
use crate::usize_into_felt;

#[derive(Debug, thiserror::Error)]
pub enum BlockHashError {
    #[error("Header is missing data (transaction_commitment / event_commitment)")]
    MissingHeaderData,
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
enum BlockHashVersion {
    V0,
    V1,
    V2,
    V3,
}

/// Validates hash of a starknet header.
/// A hash is valid if it is the result of one of the hash functions that were ever used in
/// Starknet.
pub fn validate_header(header: &BlockHeader, chain_id: &ChainId) -> Result<bool, BlockHashError> {
    for version in
        [BlockHashVersion::V3, BlockHashVersion::V2, BlockHashVersion::V1, BlockHashVersion::V0]
    {
        if calculate_block_hash_by_version(header, version, chain_id)? == header.block_hash {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Validates the body of a starknet block.
pub fn validate_body(
    body: &BlockBody,
    transaction_commitment: &TransactionCommitment,
    event_commitment: &EventCommitment,
) -> Result<bool, BlockHashError> {
    for version in
        [BlockHashVersion::V3, BlockHashVersion::V2, BlockHashVersion::V1, BlockHashVersion::V0]
    {
        let calculated_transaction_commitment =
            calculate_transaction_commitment_by_version(body, &version)?;
        if calculated_transaction_commitment != *transaction_commitment {
            continue;
        }
        let calculated_event_commitment =
            calculate_event_commitment_by_version(&body.transaction_outputs, &version);
        if calculated_event_commitment != *event_commitment {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

// Calculates hash of a starknet block by version, ignoring the block hash field in the given block.
fn calculate_block_hash_by_version(
    header: &BlockHeader,
    version: BlockHashVersion,
    chain_id: &ChainId,
) -> Result<BlockHash, BlockHashError> {
    // Can't implement as a closure because ascii_as_felt returns a Result.
    let chain_id_as_felt = if version == BlockHashVersion::V0 {
        Some(ascii_as_felt(chain_id.0.as_str())?)
    } else {
        None
    };

    Ok(BlockHash(
        HashChain::new()
        .chain(&header.block_number.0.into())
        .chain(&header.state_root.0)
        .chain_if_fn(
            || {
                if version == BlockHashVersion::V2 {
                    Some(get_chain_sequencer_address(chain_id))
                } else {
                    Some(*header.sequencer.0.key())
                }
            }
        )
        .chain_if_fn(|| {
            if version >= BlockHashVersion::V1 {
                Some(header.timestamp.0.into())
            } else {
                Some(*ZERO)
            }
        })
        .chain(&usize_into_felt(header.n_transactions.ok_or(BlockHashError::MissingHeaderData)?))
        .chain(&header.transaction_commitment.ok_or(BlockHashError::MissingHeaderData)?.0)
        .chain(&usize_into_felt(header.n_events.ok_or(BlockHashError::MissingHeaderData)?))
        .chain(&header.event_commitment.ok_or(BlockHashError::MissingHeaderData)?.0)
        .chain(&ZERO) // Not implemented Element.
        .chain(&ZERO) // Not implemented Element.
        .chain_if_fn(|| {
            chain_id_as_felt
        })

        .chain(&header.parent_hash.0).get_pedersen_hash(),
    ))
}

// Returns the transaction commitment.
fn calculate_transaction_commitment_by_version(
    block_body: &BlockBody,
    version: &BlockHashVersion,
) -> Result<TransactionCommitment, BlockHashError> {
    let transaction_patricia_leaves =
        zip(block_body.transactions.iter(), block_body.transaction_hashes.iter())
            .map(|(transaction, transaction_hash)| {
                get_transaction_leaf(transaction, transaction_hash, version)
            })
            .collect::<Result<Vec<_>, _>>()?;
    let transactions_patricia_root = calculate_root(transaction_patricia_leaves);
    Ok(TransactionCommitment(transactions_patricia_root))
}

// Returns a Patricia leaf value for a transaction.
fn get_transaction_leaf(
    transaction: &Transaction,
    transaction_hash: &TransactionHash,
    version: &BlockHashVersion,
) -> Result<StarkHash, StarknetApiError> {
    let signature = if version >= &BlockHashVersion::V3 {
        get_transaction_signature(transaction)
    } else {
        get_signature_only_from_invoke(transaction)
    };
    let signature_hash = HashChain::new().chain_iter(signature.iter()).get_pedersen_hash();
    Ok(pedersen_hash(&transaction_hash.0, &signature_hash))
}

fn get_transaction_signature(transaction: &Transaction) -> Vec<StarkFelt> {
    match transaction {
        Transaction::Declare(declare) => declare.signature().0,
        Transaction::Deploy(_) => vec![],
        Transaction::DeployAccount(deploy_account) => match deploy_account {
            DeployAccountTransaction::V1(deploy_account_v1) => {
                deploy_account_v1.signature.0.to_owned()
            }
            DeployAccountTransaction::V3(deploy_account_v3) => {
                deploy_account_v3.signature.0.to_owned()
            }
        },
        Transaction::Invoke(invoke) => invoke.signature().0,
        Transaction::L1Handler(_) => vec![],
    }
}

fn get_signature_only_from_invoke(transaction: &Transaction) -> Vec<StarkFelt> {
    if let Transaction::Invoke(invoke) = transaction { invoke.signature().0 } else { vec![] }
}

// Returns the number of the events, and the Patricia root of the events.
fn calculate_event_commitment_by_version(
    transaction_outputs: &[TransactionOutput],
    version: &BlockHashVersion,
) -> EventCommitment {
    if version < &BlockHashVersion::V1 {
        return EventCommitment(*ZERO);
    }
    let event_patricia_leaves: Vec<_> =
        transaction_outputs.iter().flat_map(|output| output.events()).map(get_event_leaf).collect();
    let event_patricia_root = calculate_root(event_patricia_leaves);
    EventCommitment(event_patricia_root)
}

// Returns a Patricia leaf value for an event.
fn get_event_leaf(event: &Event) -> StarkHash {
    let event_keys: Vec<_> = event.content.keys.iter().map(|key| key.0).collect();
    HashChain::new()
        .chain(event.from_address.0.key())
        .chain(&HashChain::new().chain_iter(event_keys.iter()).get_pedersen_hash())
        .chain(&HashChain::new().chain_iter(event.content.data.0.iter()).get_pedersen_hash())
        .get_pedersen_hash()
}

// The fixed sequencer addresses of the chains that have historic blocks with block hash version 2.
fn get_chain_sequencer_address(chain_id: &ChainId) -> StarkHash {
    match chain_id.to_string().as_str() {
        "SN_MAIN" => StarkHash::try_from(
            "0x021f4b90b0377c82bf330b7b5295820769e72d79d8acd0effa0ebde6e9988bc5",
        )
        .expect("should be a Stark felt in hex representation"),
        // TODO(yoav): Add sequencers for the rest of the supported chains that have historic blocks
        // with block hash version 2.
        _ => unimplemented!("Sequencer address for chain"),
    }
}
