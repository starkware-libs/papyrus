#[cfg(test)]
#[path = "transaction_hash_test.rs"]
mod transaction_hash_test;

use lazy_static::lazy_static;
use starknet_api::core::{calculate_contract_address, ChainId, ContractAddress};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{DeployAccountTransaction, DeployTransaction, Transaction};
use starknet_api::StarknetApiError;
use starknet_crypto::{pedersen_hash, FieldElement};

lazy_static! {
    static ref DEPLOY: StarkFelt = ascii_as_felt("deploy").unwrap();
    static ref DEPLOY_ACCOUNT: StarkFelt = ascii_as_felt("deploy_account").unwrap();
    // The first 250 bits of the Keccak256 hash on "constructor".
    static ref CONSTRUCTOR_ENTRY_POINT_SELECTOR: StarkFelt =
        StarkFelt::try_from("0x28ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194")
        .unwrap();
}

/// Calculates hash of a Starknet transaction.
pub fn get_tx_hash(tx: &Transaction, chain_id: &ChainId) -> Result<StarkHash, StarknetApiError> {
    match tx {
        Transaction::Declare(_) => unimplemented!(),
        Transaction::Deploy(deploy) => get_deploy_transaction_hash(deploy, chain_id),
        Transaction::DeployAccount(deploy_account) => {
            get_deploy_account_transaction_hash(deploy_account, chain_id)
        }
        Transaction::Invoke(_) => unimplemented!(),
        Transaction::L1Handler(_) => unimplemented!(),
    }
}

/// Validates hash of a starknet transaction.
/// A hash is valid if it is obtained from a hash calculation that was ever used in Starknet.
pub fn validate_tx_hash(
    tx: &Transaction,
    chain_id: &ChainId,
    expected_hash: StarkHash,
) -> Result<bool, StarknetApiError> {
    if get_tx_hash(tx, chain_id)? == expected_hash {
        return Ok(true);
    }
    let deprecated_hashes = match tx {
        Transaction::Declare(_) => unimplemented!(),
        Transaction::Deploy(deploy) => {
            vec![get_deprecated_deploy_transaction_hash(deploy, chain_id)?]
        }
        Transaction::DeployAccount(_) => {
            vec![]
        }
        Transaction::Invoke(_) => unimplemented!(),
        Transaction::L1Handler(_) => unimplemented!(),
    };
    Ok(deprecated_hashes.contains(&expected_hash))
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

    // Chains a felt to the hash chain if a condition is true.
    pub fn chain_if(self, felt: &StarkFelt, condition: bool) -> Self {
        if condition { self.chain(felt) } else { self }
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

fn get_deploy_account_transaction_hash(
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

fn get_deploy_transaction_hash(
    tx: &DeployTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_deploy_transaction_hash(tx, chain_id, false)
}

fn get_deprecated_deploy_transaction_hash(
    tx: &DeployTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_deploy_transaction_hash(tx, chain_id, true)
}

fn get_common_deploy_transaction_hash(
    tx: &DeployTransaction,
    chain_id: &ChainId,
    is_deprecated: bool,
) -> Result<StarkHash, StarknetApiError> {
    let contract_address = calculate_contract_address(
        tx.contract_address_salt,
        tx.class_hash,
        &tx.constructor_calldata,
        ContractAddress::from(0_u8),
    )?;

    Ok(PedersenHashChain::new()
        .chain(&DEPLOY)
        .chain_if(&tx.version.0, !is_deprecated)
        .chain(contract_address.0.key())
        .chain(&CONSTRUCTOR_ENTRY_POINT_SELECTOR)
        .chain(&PedersenHashChain::new().chain_iter(tx.constructor_calldata.0.iter()).get_hash())
        .chain_if(&StarkFelt::from(0_u8), !is_deprecated)
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .get_hash())
}
