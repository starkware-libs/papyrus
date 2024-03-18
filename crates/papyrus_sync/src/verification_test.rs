use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockSignature};
use starknet_api::core::{SequencerPublicKey, StateDiffCommitment};
use starknet_api::crypto::{PublicKey, Signature};
use starknet_api::hash::{PoseidonHash, StarkFelt};
use starknet_api::stark_felt;

use crate::verification::{Verifier, VerifierImpl};

#[test]
fn verify_block_signature() {
    let block_hash =
        BlockHash(stark_felt!("0x7d5db04c5ca2aea828180dc441afb1580e3cee7547a3567ced3aa5bb8b273c0"));
    let state_diff_commitment = StateDiffCommitment(PoseidonHash(stark_felt!(
        "0x64689c12248e1110af4b3af0e2b43cd51ad13e8855f10e37669e2a4baf919c6"
    )));
    let signature = BlockSignature(Signature {
        r: stark_felt!("0x1b382bbfd693011c9b7692bc932b23ed9c288deb27c8e75772e172abbe5950c"),
        s: stark_felt!("0xbe4438085057e1a7c704a0da3b30f7b8340fe3d24c86772abfd24aa597e42"),
    });
    let sequencer_pub_key = SequencerPublicKey(PublicKey(stark_felt!(
        "0x48253ff2c3bed7af18bde0b611b083b39445959102d4947c51c4db6aa4f4e58"
    )));
    assert_eq!(
        true,
        VerifierImpl::verify_signature(
            &block_hash,
            &state_diff_commitment,
            &signature,
            &sequencer_pub_key
        )
        .unwrap()
    );
}
