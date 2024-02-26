use papyrus_common::block_hash::{
    self,
    calculate_block_hash_by_version,
    get_event_commitment,
    get_transaction_commitment,
    BlockHashVersion,
};
use starknet_api::block::{
    verify_block_signature,
    BlockBody,
    BlockHash,
    BlockHeader,
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
    fn validate_header(
        header: &BlockHeader,
        chain_id: &ChainId,
        block_hash_version: BlockHashVersion,
    ) -> VerificationResult;
    /// Verifies that the block body is valid.
    fn validate_body(
        body: &BlockBody,
        transaction_commitment: &TransactionCommitment,
        event_commitment: &EventCommitment,
        block_hash_version: &BlockHashVersion,
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

    fn validate_header(
        header: &BlockHeader,
        chain_id: &ChainId,
        block_hash_version: BlockHashVersion,
    ) -> VerificationResult {
        let block_hash = calculate_block_hash_by_version(header, block_hash_version, chain_id)
            .map_err(VerificationError::HeaderVerificationError)?;
        Ok(block_hash == header.block_hash)
    }

    fn validate_body(
        body: &BlockBody,
        expected_transaction_commitment: &TransactionCommitment,
        expected_event_commitment: &EventCommitment,
        block_hash_version: &block_hash::BlockHashVersion,
    ) -> VerificationResult {
        let tx_commitment = get_transaction_commitment(body, block_hash_version)
            .map_err(VerificationError::BodyVerificationError)?;
        if tx_commitment != *expected_transaction_commitment {
            return Ok(false);
        }
        let event_commitment = get_event_commitment(&body.transaction_outputs, block_hash_version);
        if event_commitment != *expected_event_commitment {
            return Ok(false);
        }
        Ok(true)
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
