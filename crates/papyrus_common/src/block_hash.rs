#[cfg(test)]
#[path = "block_hash_test.rs"]
mod block_hash_test;

use std::iter::zip;

use starknet_api::block::{Block, BlockBody};
use starknet_api::core::ChainId;
use starknet_api::hash::{pedersen_hash, StarkFelt, StarkHash};
use starknet_api::transaction::{Event, Transaction, TransactionHash, TransactionOutput};
use starknet_api::StarknetApiError;

use crate::patricia_hash_tree::calculate_root;
use crate::transaction_hash::{ascii_as_felt, PedersenHashChain, ZERO};

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
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
        if calculate_block_hash_by_version(block, version, chain_id)? == block.header.block_hash.0 {
            return Ok(true);
        }
    }
    Ok(false)
}

// Calculates hash of a starknet block by version, ignoring the block hash field in the given block.
fn calculate_block_hash_by_version(
    block: &Block,
    version: BlockHashVersion,
    chain_id: &ChainId,
) -> Result<StarkFelt, StarknetApiError> {
    let (n_transactions, transactions_patricia_root) =
        get_transactions_hash_data(&block.body, &version)?;

    let (n_events, events_patricia_root) =
        get_events_hash_data(&block.body.transaction_outputs, &version);

    Ok(PedersenHashChain::new()
        .chain(&block.header.block_number.0.into())
        .chain(&block.header.state_root.0)
        .chain_if_else(
            &get_chain_sequencer_address(chain_id),
            block.header.sequencer.0.key(),
            version == BlockHashVersion::V2,
        )
        .chain_if_else(&block.header.timestamp.0.into(), &ZERO, version >= BlockHashVersion::V1)
        .chain(&n_transactions)
        .chain(&transactions_patricia_root)
        .chain(&n_events)
        .chain(&events_patricia_root)
        .chain(&ZERO) // Not implemented Element.
        .chain(&ZERO) // Not implemented Element.
        .chain_if(&ascii_as_felt(chain_id.0.as_str())?, version == BlockHashVersion::V0)
        .chain(&block.header.parent_hash.0)
        .get_hash())
}

// Returns the number of the transactions, and the Patricia root of the transactions.
fn get_transactions_hash_data(
    block_body: &BlockBody,
    version: &BlockHashVersion,
) -> Result<(StarkFelt, StarkFelt), StarknetApiError> {
    let n_transactions = usize_into_felt(block_body.transactions.len());
    let transaction_patricia_leaves =
        zip(block_body.transactions.iter(), block_body.transaction_hashes.iter())
            .map(|(transaction, transaction_hash)| {
                get_transaction_leaf(transaction, transaction_hash, version)
            })
            .collect::<Result<Vec<_>, _>>()?;
    let transactions_patricia_root = calculate_root(transaction_patricia_leaves);
    Ok((n_transactions, transactions_patricia_root))
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
    let signature_hash = PedersenHashChain::new().chain_iter(signature.iter()).get_hash();
    Ok(pedersen_hash(&transaction_hash.0, &signature_hash))
}

fn get_transaction_signature(transaction: &Transaction) -> Vec<StarkFelt> {
    match transaction {
        Transaction::Declare(declare) => declare.signature().0,
        Transaction::Deploy(_) => vec![],
        Transaction::DeployAccount(deploy_account) => deploy_account.signature.0.to_owned(),
        Transaction::Invoke(invoke) => invoke.signature().0,
        Transaction::L1Handler(_) => vec![],
    }
}

fn get_signature_only_from_invoke(transaction: &Transaction) -> Vec<StarkFelt> {
    if let Transaction::Invoke(invoke) = transaction { invoke.signature().0 } else { vec![] }
}

// Returns the number of the events, and the Patricia root of the events.
fn get_events_hash_data(
    transaction_outputs: &[TransactionOutput],
    version: &BlockHashVersion,
) -> (StarkFelt, StarkFelt) {
    if version < &BlockHashVersion::V1 {
        return (*ZERO, *ZERO);
    }
    let event_patricia_leaves: Vec<_> =
        transaction_outputs.iter().flat_map(|output| output.events()).map(get_event_leaf).collect();
    (usize_into_felt(event_patricia_leaves.len()), calculate_root(event_patricia_leaves))
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

// The fixed sequencer addresses that were in use.
fn get_chain_sequencer_address(chain_id: &ChainId) -> StarkHash {
    match chain_id.to_string().as_str() {
        "SN_MAIN" => StarkHash::try_from(
            "0x021f4b90b0377c82bf330b7b5295820769e72d79d8acd0effa0ebde6e9988bc5",
        )
        .expect("should be a Stark felt in hex representation"),
        // TODO(yoav): Add sequencers for the rest of the supported chains.
        _ => unimplemented!("Sequencer address for chain"),
    }
}

fn usize_into_felt(u: usize) -> StarkFelt {
    u128::try_from(u).expect("Expect at most 128 bits").into()
}
