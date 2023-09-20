#[cfg(test)]
#[path = "block_hash_test.rs"]
mod block_hash_test;

use std::iter::zip;

use starknet_api::block::{Block, BlockBody, BlockBodyCommitments, BlockHeader};
use starknet_api::core::ChainId;
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
use crate::transaction_hash::{ascii_as_felt, PedersenHashChain, ZERO};

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
enum BlockHashVersion {
    V0,
    V1,
    V2,
    V3,
}

/// Validates hash of a starknet block.
/// A hash is valid if it is the result of one of the hash functions that were ever used in
/// Starknet.
pub fn validate_block_hash(block: &Block, chain_id: &ChainId) -> Result<bool, StarknetApiError> {
    for version in
        [BlockHashVersion::V3, BlockHashVersion::V2, BlockHashVersion::V1, BlockHashVersion::V0]
    {
        if calculate_block_hash_by_version(&block.header, &block.commitments, version, chain_id)?
            == block.header.block_hash.0
        {
            return Ok(true);
        }
    }
    Ok(false)
}

// Calculates hash of a starknet block by version, ignoring the block hash field in the given block
// header.
fn calculate_block_hash_by_version(
    block_header: &BlockHeader,
    block_commitments: &BlockBodyCommitments,
    version: BlockHashVersion,
    chain_id: &ChainId,
) -> Result<StarkFelt, StarknetApiError> {
    let sequencer = if version == BlockHashVersion::V2 {
        get_chain_sequencer_address(chain_id)
    } else {
        block_header.sequencer.0.key().to_owned()
    };

    Ok(PedersenHashChain::new()
        .chain(&block_header.block_number.0.into())
        .chain(&block_header.state_root.0)
        .chain(&sequencer)
        .chain_if_else(&block_header.timestamp.0.into(), &ZERO, version >= BlockHashVersion::V1)
        .chain(&block_header.n_transactions.into())
        .chain(&block_commitments.transactions_commitment)
        .chain_if_else(&block_header.n_events.into(), &ZERO, version >= BlockHashVersion::V1)
        .chain_if_else(&block_commitments.events_commitment, &ZERO, version >= BlockHashVersion::V1)
        .chain(&ZERO) // Not implemented element.
        .chain(&ZERO) // Not implemented element.
        .chain_if(&ascii_as_felt(chain_id.0.as_str())?, version == BlockHashVersion::V0)
        .chain(&block_header.parent_hash.0)
        .get_hash())
}

// Calculates the commitments according to the fittest block hash version. However, the result does
// not guarantee that the block hash is valid.
pub fn calculate_block_commitments(
    block_header: &BlockHeader,
    block_body: &BlockBody,
) -> Result<BlockBodyCommitments, StarknetApiError> {
    let events_commitment = get_events_commitment(&block_body.transaction_outputs);

    // Try to reach the given block hash by v3 transactions commitment.
    let v3_transactions_commitment = get_transactions_commitment(block_body, false)?;
    let v3_block_commitments = BlockBodyCommitments {
        transactions_commitment: v3_transactions_commitment,
        events_commitment,
    };
    let v3_block_hash = calculate_block_hash_by_version(
        block_header,
        &v3_block_commitments,
        BlockHashVersion::V3,
        // The chain id is not used for version 3.
        &ChainId("".to_owned()),
    )?;
    if v3_block_hash == block_header.block_hash.0 {
        return Ok(v3_block_commitments);
    }

    // Assume we can reach the given block hash by deprecated transactions commitment with some
    // deprecated block hash versions.
    let deprecated_transactions_commitment = get_transactions_commitment(block_body, true)?;
    Ok(BlockBodyCommitments {
        transactions_commitment: deprecated_transactions_commitment,
        events_commitment,
    })
}

// Returns the Patricia root of the transactions.
fn get_transactions_commitment(
    block_body: &BlockBody,
    deprecated_version: bool,
) -> Result<StarkFelt, StarknetApiError> {
    let transaction_patricia_leaves =
        zip(block_body.transactions.iter(), block_body.transaction_hashes.iter())
            .map(|(transaction, transaction_hash)| {
                get_transaction_leaf(transaction, transaction_hash, deprecated_version)
            })
            .collect::<Result<Vec<_>, _>>()?;
    Ok(calculate_root(transaction_patricia_leaves))
}

// Returns a Patricia leaf value for a transaction.
fn get_transaction_leaf(
    transaction: &Transaction,
    transaction_hash: &TransactionHash,
    deprecated_version: bool,
) -> Result<StarkHash, StarknetApiError> {
    let signature = if deprecated_version {
        get_signature_only_from_invoke(transaction)
    } else {
        get_transaction_signature(transaction)
    };
    let signature_hash = PedersenHashChain::new().chain_iter(signature.iter()).get_hash();
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

// Returns the Patricia root of the events.
fn get_events_commitment(transaction_outputs: &[TransactionOutput]) -> StarkFelt {
    let event_patricia_leaves: Vec<_> =
        transaction_outputs.iter().flat_map(|output| output.events()).map(get_event_leaf).collect();
    calculate_root(event_patricia_leaves)
}

// Returns a Patricia leaf value for an event.
fn get_event_leaf(event: &Event) -> StarkHash {
    let event_keys: Vec<_> = event.content.keys.iter().map(|key| key.0).collect();
    PedersenHashChain::new()
        .chain(event.from_address.0.key())
        .chain(&PedersenHashChain::new().chain_iter(event_keys.iter()).get_hash())
        .chain(&PedersenHashChain::new().chain_iter(event.content.data.0.iter()).get_hash())
        .get_hash()
}

// The fixed sequencer addresses of the chains that have historic blocks with block hash version 2.
fn get_chain_sequencer_address(chain_id: &ChainId) -> StarkHash {
    match chain_id.0.as_str() {
        "SN_MAIN" => StarkHash::try_from(
            "0x021f4b90b0377c82bf330b7b5295820769e72d79d8acd0effa0ebde6e9988bc5",
        )
        .expect("should be a Stark felt in hex representation"),
        // TODO(yoav): Add sequencers for the rest of the supported chains that have historic blocks
        // with block hash version 2.
        _ => unimplemented!("Sequencer address for chain"),
    }
}
