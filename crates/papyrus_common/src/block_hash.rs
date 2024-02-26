#[cfg(test)]
#[path = "block_hash_test.rs"]
mod block_hash_test;
use std::iter::zip;

use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum BlockHashVersion {
    V0,
    V1,
    V2,
    V3,
}

#[derive(Debug, thiserror::Error)]
pub enum BlockHashError {
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error("Missing transaction commitment")]
    MissingTransactionCommitment,
    #[error("Missing event commitment")]
    MissingEventCommitment,
    #[error("Missing number of transactions")]
    MissingNumTransactions,
    #[error("Missing number of events")]
    MissingNumEvents,
}

// Calculates hash of a starknet block by version, ignoring the block hash field in the given block.
pub fn calculate_block_hash_by_version(
    header: &BlockHeader,
    version: BlockHashVersion,
    chain_id: &ChainId,
) -> Result<BlockHash, BlockHashError> {
    let n_transactions: StarkFelt =
        u64::try_from(header.n_transactions.ok_or(BlockHashError::MissingNumTransactions)?)
            .expect("Failed to convert usize to u64.")
            .into();
    let transaction_commitment =
        header.transaction_commitment.ok_or(BlockHashError::MissingTransactionCommitment)?;

    let n_events: StarkFelt =
        u64::try_from(header.n_events.ok_or(BlockHashError::MissingNumEvents)?)
            .expect("Failed to convert usize to u64.")
            .into();
    let event_commitment = header.event_commitment.ok_or(BlockHashError::MissingEventCommitment)?;
    Ok(BlockHash(
        HashChain::new()
        .chain(&header.block_number.0.into())
        .chain(&header.state_root.0)
        .chain_if_else(
            &get_chain_sequencer_address(chain_id),
            header.sequencer.0.key(),
            version == BlockHashVersion::V2,
        )
        .chain_if_else(&header.timestamp.0.into(), &ZERO, version >= BlockHashVersion::V1)
        .chain(&n_transactions)
        .chain(&transaction_commitment.0)
        .chain(&n_events)
        .chain(&event_commitment.0)
        .chain(&ZERO) // Not implemented Element.
        .chain(&ZERO) // Not implemented Element.
        .chain_if(&ascii_as_felt(chain_id.0.as_str())?, version == BlockHashVersion::V0)
        .chain(&header.parent_hash.0).get_pedersen_hash(),
    ))
}

/// Returns the Patricia root of the transactions.
pub fn get_transaction_commitment(
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

// Returns the Patricia root of the events.
pub fn get_event_commitment(
    transaction_outputs: &[TransactionOutput],
    version: &BlockHashVersion,
) -> EventCommitment {
    if version < &BlockHashVersion::V1 {
        return EventCommitment(*ZERO);
    }
    let event_patricia_leaves: Vec<_> =
        transaction_outputs.iter().flat_map(|output| output.events()).map(get_event_leaf).collect();
    EventCommitment(calculate_root(event_patricia_leaves))
}

// TODO(yair): Once 0.13.1 arrives to mainnet, update the json files to include the missing fields.
pub fn fill_missing_header_fields(block: &mut Block, version: BlockHashVersion) {
    if block.header.n_transactions.is_none() {
        block.header.n_transactions = Some(block.body.transactions.len());
    }
    if block.header.transaction_commitment.is_none() {
        block.header.transaction_commitment =
            Some(get_transaction_commitment(&block.body, &version).unwrap());
    }
    if block.header.event_commitment.is_none() {
        block.header.event_commitment =
            Some(get_event_commitment(&block.body.transaction_outputs, &version));
    }
    if block.header.n_events.is_none() {
        block.header.n_events = Some(
            block
                .body
                .transaction_outputs
                .iter()
                .fold(0, |acc, tx_output| acc + tx_output.events().len()),
        );
    }
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
