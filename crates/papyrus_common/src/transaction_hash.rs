#[cfg(test)]
#[path = "transaction_hash_test.rs"]
mod transaction_hash_test;

use lazy_static::lazy_static;
use starknet_api::core::{calculate_contract_address, ChainId, ContractAddress};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::DeployAccountTransaction;
use starknet_api::StarknetApiError;
use starknet_crypto::{pedersen_hash, FieldElement};

lazy_static! {
    static ref DEPLOY_ACCOUNT: StarkFelt = ascii_as_felt("deploy_account").unwrap();
}

// Represents an intermediate calculation of Pedersen hash chain.
struct PedersenHashChain {
    current_hash: FieldElement,
    length: u128,
}

impl PedersenHashChain {
    pub fn new() -> PedersenHashChain {
        PedersenHashChain { current_hash: FieldElement::ZERO, length: 0 }
    }

    // Chains a felt to the hash chain.
    pub fn chain(self, felt: &StarkFelt) -> Self {
        let new_hash = pedersen_hash(&self.current_hash, &FieldElement::from(*felt));
        Self { current_hash: new_hash, length: self.length + 1 }
    }

    // Chains many felts to the hash chain.
    pub fn chain_iter<'a>(self, felts: impl Iterator<Item = &'a StarkFelt>) -> Self {
        felts.fold(self, |current, felt| current.chain(felt))
    }

    // Returns the hash of the chained felts, hashed with the length of the chain.
    pub fn get_hash(&self) -> StarkHash {
        let final_hash = pedersen_hash(&self.current_hash, &FieldElement::from(self.length));
        StarkHash::from(final_hash)
    }
}

fn ascii_as_felt(ascii_str: &str) -> Result<StarkFelt, StarknetApiError> {
    StarkFelt::try_from(hex::encode(ascii_str).as_str())
}

pub fn get_deploy_account_transaction_hash(
    tx: &DeployAccountTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    let calldata_hash = PedersenHashChain::new()
        .chain(&tx.class_hash.0)
        .chain(&tx.contract_address_salt.0)
        .chain_iter(tx.constructor_calldata.0.iter())
        .get_hash();

    let contract_address = calculate_contract_address(
        tx.contract_address_salt,
        tx.class_hash,
        &tx.constructor_calldata,
        ContractAddress::from(0_u8),
    )?;

    Ok(PedersenHashChain::new()
        .chain(&DEPLOY_ACCOUNT)
        .chain(&tx.version.0)
        .chain(contract_address.0.key())
        .chain(&StarkFelt::from(0_u8))
        .chain(&calldata_hash)
        .chain(&tx.max_fee.0.into())
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&tx.nonce.0)
        .get_hash())
}
