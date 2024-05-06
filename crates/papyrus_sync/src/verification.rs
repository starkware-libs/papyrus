use std::collections::{BTreeMap, HashMap};

use lazy_static::lazy_static;
use papyrus_common::block_hash::BlockHashError;
use papyrus_common::class_hash::{self, calculate_class_hash, calculate_deprecated_class_hash};
use papyrus_common::{block_hash, state_diff_commitment, transaction_hash, TransactionOptions};
use starknet_api::block::{
    verify_block_signature,
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockSignature,
};
use starknet_api::core::{
    ChainId,
    ClassHash,
    EventCommitment,
    GlobalRoot,
    SequencerPublicKey,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, ThinStateDiff};
use starknet_api::transaction::{Event, Transaction, TransactionHash};
use tracing::{debug, warn};

lazy_static! {
    // The block hash versions for each chain.
    static ref BLOCK_HASH_VERSIONS: HashMap<ChainId, BTreeMap<BlockNumber, block_hash::BlockHashVersion>> =
        HashMap::from(
            [
                (
                    ChainId("SN_MAIN".to_string()),
                    BTreeMap::from([
                        (BlockNumber(0), block_hash::BlockHashVersion::V0),
                        (BlockNumber(833), block_hash::BlockHashVersion::V1),
                        (BlockNumber(1466), block_hash::BlockHashVersion::V2),
                        (BlockNumber(61394), block_hash::BlockHashVersion::V3),
                    ]),
                ),
                (
                    ChainId("SN_SEPOLIA".to_string()),
                    BTreeMap::from([
                        (BlockNumber(0), block_hash::BlockHashVersion::V3),
                    ]),
                ),
                (
                    ChainId("SN_SEPOLIA_INTEGRATION".to_string()),
                    BTreeMap::from([
                        (BlockNumber(0), block_hash::BlockHashVersion::V3),
                    ]),
                ),
            ]
        );
}

pub fn get_block_hash_version(
    chain_id: &ChainId,
    block_number: &BlockNumber,
) -> block_hash::BlockHashVersion {
    let bn_to_version = BLOCK_HASH_VERSIONS.get(chain_id).expect("Chain ID not found");
    for (bn, version) in bn_to_version.iter().rev() {
        if block_number >= bn {
            return *version;
        }
    }
    unreachable!("Shouldn't reach here");
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error(transparent)]
    BodyVerificationError(block_hash::BlockHashError),
    #[error(transparent)]
    DeprecatedClassVerificationError(#[from] class_hash::DeprecatedClassHashCalculationError),
    #[error(transparent)]
    HeaderVerificationError(block_hash::BlockHashError),
    #[error(transparent)]
    SignatureVerificationError(#[from] starknet_api::block::BlockVerificationError),
    #[error(transparent)]
    TransactionVerificationError(#[from] starknet_api::StarknetApiError),
}

pub type VerificationResult = Result<bool, VerificationError>;

/// A trait for verifying Starknet objects.
pub trait Verifier {
    /// Verifies the block signature.
    // TODO: Once the state_diff_commitment is added to the block hash, remove it.
    fn verify_signature(
        block_hash: &BlockHash,
        state_diff_commitment: &StateDiffCommitment,
        signature: &BlockSignature,
        sequencer_pub_key: &SequencerPublicKey,
    ) -> VerificationResult;
    /// Verifies that the header is valid.
    fn validate_header(header: &BlockHeader, chain_id: &ChainId) -> VerificationResult;
    /// Verifies that the block body is valid.
    fn validate_body<'a>(
        block_number: &BlockNumber,
        chain_id: &ChainId,
        transactions: &[Transaction],
        events: impl Iterator<Item = &'a Event>,
        transaction_hashes: &[TransactionHash],
        expected_transaction_commitment: &TransactionCommitment,
        expected_event_commitment: &EventCommitment,
    ) -> VerificationResult;
    /// Verifies that the transaction is valid.
    fn validate_transaction(
        transaction: &Transaction,
        block_number: &BlockNumber,
        chain_id: &ChainId,
        transaction_hash: &TransactionHash,
    ) -> VerificationResult;
    /// Verifies that the state diff is valid.
    fn validate_state_diff(
        state_diff: &ThinStateDiff,
        expected_state_diff_commitment: &StateDiffCommitment,
    ) -> VerificationResult;
    /// Verifies that Cairo1 class is valid.
    fn validate_class(class: &ContractClass, class_hash: &ClassHash) -> VerificationResult;
    /// Verifies that Cairo0 class is valid.
    fn validate_deprecated_class(
        class: &mut DeprecatedContractClass,
        class_hash: &ClassHash,
    ) -> VerificationResult;
}

pub struct CentralSourceVerifier;

impl Verifier for CentralSourceVerifier {
    fn verify_signature(
        block_hash: &BlockHash,
        state_diff_commitment: &StateDiffCommitment,
        signature: &BlockSignature,
        sequencer_pub_key: &SequencerPublicKey,
    ) -> VerificationResult {
        // TODO(yair): Change verify_block_signature in starknet_api to accept StateDiffCommitment
        // instead of GlobalRoot.
        let state_diff_commitment = GlobalRoot(state_diff_commitment.0.0);
        verify_block_signature(sequencer_pub_key, signature, &state_diff_commitment, block_hash)
            .map_err(VerificationError::SignatureVerificationError)
    }

    fn validate_header(header: &BlockHeader, chain_id: &ChainId) -> VerificationResult {
        let block_hash_version = get_block_hash_version(chain_id, &header.block_number);
        let calculated_block_hash =
            block_hash::calculate_block_hash_by_version(header, block_hash_version, chain_id);
        if let Err(BlockHashError::MissingHeaderData) = calculated_block_hash {
            warn!(
                "Skipping validation of block header {} due to missing commitments data",
                header.block_number.0
            );
            return Ok(true);
        }
        let calculated_block_hash =
            calculated_block_hash.map_err(VerificationError::HeaderVerificationError)?;
        if calculated_block_hash != header.block_hash {
            debug!(
                "Header {} validation failed: calculated block hash: {:?}, header block hash: {:?}",
                header.block_number.0, calculated_block_hash, header.block_hash
            );
        }
        Ok(calculated_block_hash == header.block_hash)
    }

    fn validate_body<'a>(
        block_number: &BlockNumber,
        chain_id: &ChainId,
        transactions: &[Transaction],
        events: impl Iterator<Item = &'a Event>,
        transaction_hashes: &[TransactionHash],
        expected_transaction_commitment: &TransactionCommitment,
        expected_event_commitment: &EventCommitment,
    ) -> VerificationResult {
        if expected_transaction_commitment == &TransactionCommitment::default()
            && expected_event_commitment == &EventCommitment::default()
        {
            debug!(
                "Skipping validation of block body {} due to missing commitments data",
                block_number.0
            );
            return Ok(true);
        }
        let block_hash_version = get_block_hash_version(chain_id, block_number);
        // TODO(yair): parallelize the commitment calculations using rayon.
        let calculated_transaction_commitment =
            block_hash::calculate_transaction_commitment_by_version(
                transactions,
                transaction_hashes,
                &block_hash_version,
            )
            .map_err(VerificationError::BodyVerificationError)?;
        if calculated_transaction_commitment != *expected_transaction_commitment {
            debug!(
                "Transaction commitment validation failed: calculated: {:?}, expected: {:?}",
                calculated_transaction_commitment, expected_transaction_commitment
            );
            return Ok(false);
        }
        let calculated_event_commitment =
            block_hash::calculate_event_commitment_by_version(events, &block_hash_version);
        if calculated_event_commitment != *expected_event_commitment {
            debug!(
                "Event commitment validation failed: calculated: {:?}, expected: {:?}",
                calculated_event_commitment, expected_event_commitment
            );
            return Ok(false);
        }

        // TODO(yair): parallelize the transaction validations using rayon.
        for (tx, hash) in transactions.iter().zip(transaction_hashes.iter()) {
            if !Self::validate_transaction(tx, block_number, chain_id, hash)? {
                debug!("Transaction validation failed: {:?}", tx);
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn validate_transaction(
        transaction: &Transaction,
        block_number: &BlockNumber,
        chain_id: &ChainId,
        transaction_hash: &TransactionHash,
    ) -> VerificationResult {
        transaction_hash::validate_transaction_hash(
            transaction,
            block_number,
            chain_id,
            *transaction_hash,
            &TransactionOptions { only_query: false },
        )
        .map_err(VerificationError::TransactionVerificationError)
    }

    fn validate_state_diff(
        state_diff: &ThinStateDiff,
        expected_state_diff_commitment: &StateDiffCommitment,
    ) -> VerificationResult {
        let calculated_commitment = state_diff_commitment::calculate_state_diff_commitment(
            state_diff,
            state_diff_commitment::StateDiffVersion::V0,
        );
        if calculated_commitment != *expected_state_diff_commitment {
            return Ok(false);
        }
        Ok(true)
    }

    fn validate_class(class: &ContractClass, class_hash: &ClassHash) -> VerificationResult {
        let calculated_class_hash = calculate_class_hash(class);
        Ok(calculated_class_hash == *class_hash)
    }

    fn validate_deprecated_class(
        class: &mut DeprecatedContractClass,
        class_hash: &ClassHash,
    ) -> VerificationResult {
        let calculated_class_hash = calculate_deprecated_class_hash(class)?;
        Ok(calculated_class_hash == *class_hash)
    }
}
