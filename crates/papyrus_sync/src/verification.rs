use std::collections::{BTreeMap, HashMap};

use lazy_static::lazy_static;
use papyrus_common::block_hash;
use starknet_api::block::{
    verify_block_signature,
    BlockBody,
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
use starknet_api::state::{ContractClass, StateDiff};

lazy_static! {
    // The block hash versions for each chain.
    static ref BLOCK_HASH_VERSIONS: HashMap<ChainId, BTreeMap<BlockNumber, block_hash::BlockHashVersion>> =
        HashMap::from([(
            ChainId("SN_MAIN".to_string()),
            BTreeMap::from([
                (BlockNumber(0), block_hash::BlockHashVersion::V0),
                (BlockNumber(833), block_hash::BlockHashVersion::V3),
            ])
        )]);
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
    SignatureVerificationError(#[from] starknet_api::block::BlockVerificationError),
    #[error(transparent)]
    HeaderVerificationError(block_hash::BlockHashError),
}

pub type VerificationResult = Result<bool, VerificationError>;

/// A trait for verifying the validity of Starknet objects.
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
    fn validate_body(
        body: &BlockBody,
        transaction_commitment: &TransactionCommitment,
        event_commitment: &EventCommitment,
    ) -> VerificationResult;
    /// Verifies that the state diff is valid.
    fn validate_state_diff(
        state_diff: &StateDiff,
        state_diff_commitment: &StateDiffCommitment,
    ) -> VerificationResult;
    /// Verifies that Cairo1 class is valid.
    fn validate_class(class: &ContractClass, class_hash: &ClassHash) -> VerificationResult;
    /// Verifies that Cairo0 class is valid.
    fn validate_deprecated_class(
        class: &DeprecatedContractClass,
        class_hash: &ClassHash,
    ) -> VerificationResult;
}

pub struct VerifierImpl;

impl Verifier for VerifierImpl {
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
            block_hash::calculate_block_hash_by_version(header, block_hash_version, chain_id)
                .map_err(VerificationError::HeaderVerificationError)?;
        Ok(calculated_block_hash == header.block_hash)
    }

    fn validate_body(
        body: &BlockBody,
        expected_transaction_commitment: &TransactionCommitment,
        expected_event_commitment: &EventCommitment,
    ) -> VerificationResult {
        block_hash::validate_body(body, expected_transaction_commitment, expected_event_commitment)
            .map_err(VerificationError::BodyVerificationError)
    }

    fn validate_state_diff(
        _state_diff: &StateDiff,
        _state_diff_commitment: &StateDiffCommitment,
    ) -> VerificationResult {
        todo!()
    }

    fn validate_class(_class: &ContractClass, _class_hash: &ClassHash) -> VerificationResult {
        todo!()
    }

    fn validate_deprecated_class(
        _class: &DeprecatedContractClass,
        _class_hash: &ClassHash,
    ) -> VerificationResult {
        todo!()
    }
}
