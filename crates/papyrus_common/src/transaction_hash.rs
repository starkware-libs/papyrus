#[cfg(test)]
#[path = "transaction_hash_test.rs"]
mod transaction_hash_test;

use lazy_static::lazy_static;
use starknet_api::core::{calculate_contract_address, ChainId, ContractAddress};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    DeployTransaction,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    L1HandlerTransaction,
    Transaction,
};
use starknet_api::StarknetApiError;
use starknet_crypto::{pedersen_hash, FieldElement};

lazy_static! {
    static ref DECLARE: StarkFelt = ascii_as_felt("declare").unwrap();
    static ref DEPLOY: StarkFelt = ascii_as_felt("deploy").unwrap();
    static ref DEPLOY_ACCOUNT: StarkFelt = ascii_as_felt("deploy_account").unwrap();
    static ref INVOKE: StarkFelt = ascii_as_felt("invoke").unwrap();
    static ref L1_HANDLER: StarkFelt = ascii_as_felt("l1_handler").unwrap();
    // The first 250 bits of the Keccak256 hash on "constructor".
    // The correctness of this constant is enforced by a test.
    static ref CONSTRUCTOR_ENTRY_POINT_SELECTOR: StarkFelt =
        StarkFelt::try_from("0x28ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194")
        .unwrap();

    pub(crate) static ref ZERO: StarkFelt = StarkFelt::from(0_u8);
    static ref ONE: StarkFelt = StarkFelt::from(1_u8);
    static ref TWO: StarkFelt = StarkFelt::from(2_u8);
}

/// Calculates hash of a Starknet transaction.
pub fn get_transaction_hash(
    transaction: &Transaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    match transaction {
        Transaction::Declare(declare) => match declare {
            DeclareTransaction::V0(declare_v0) => {
                get_declare_transaction_v0_hash(declare_v0, chain_id)
            }
            DeclareTransaction::V1(declare_v1) => {
                get_declare_transaction_v1_hash(declare_v1, chain_id)
            }
            DeclareTransaction::V2(declare_v2) => {
                get_declare_transaction_v2_hash(declare_v2, chain_id)
            }
        },
        Transaction::Deploy(deploy) => get_deploy_transaction_hash(deploy, chain_id),
        Transaction::DeployAccount(deploy_account) => {
            get_deploy_account_transaction_hash(deploy_account, chain_id)
        }
        Transaction::Invoke(invoke) => match invoke {
            InvokeTransaction::V0(invoke_v0) => get_invoke_transaction_v0_hash(invoke_v0, chain_id),
            InvokeTransaction::V1(invoke_v1) => get_invoke_transaction_v1_hash(invoke_v1, chain_id),
        },
        Transaction::L1Handler(l1_handler) => get_l1_handler_transaction_hash(l1_handler, chain_id),
    }
}

/// Validates hash of a starknet transaction.
/// A hash is valid if it is the result of one of the hash functions that were ever used in
/// Starknet.
pub fn validate_transaction_hash(
    transaction: &Transaction,
    chain_id: &ChainId,
    expected_hash: StarkHash,
) -> Result<bool, StarknetApiError> {
    let mut possible_hashes = match transaction {
        Transaction::Declare(_) => vec![],
        Transaction::Deploy(deploy) => {
            vec![get_deprecated_deploy_transaction_hash(deploy, chain_id)?]
        }
        Transaction::DeployAccount(_) => vec![],
        Transaction::Invoke(invoke) => match invoke {
            InvokeTransaction::V0(invoke_v0) => {
                vec![get_deprecated_invoke_transaction_v0_hash(invoke_v0, chain_id)?]
            }
            InvokeTransaction::V1(_) => vec![],
        },
        Transaction::L1Handler(l1_handler) => {
            vec![get_deprecated_l1_handler_transaction_hash(l1_handler, chain_id)?]
        }
    };
    possible_hashes.push(get_transaction_hash(transaction, chain_id)?);
    Ok(possible_hashes.contains(&expected_hash))
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
    transaction: &DeployAccountTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    let calldata_hash = PedersenHashChain::new()
        .chain(&transaction.class_hash.0)
        .chain(&transaction.contract_address_salt.0)
        .chain_iter(transaction.constructor_calldata.0.iter())
        .get_hash();

    let contract_address = calculate_contract_address(
        transaction.contract_address_salt,
        transaction.class_hash,
        &transaction.constructor_calldata,
        ContractAddress::from(0_u8),
    )?;

    Ok(PedersenHashChain::new()
        .chain(&DEPLOY_ACCOUNT)
        .chain(&transaction.version.0)
        .chain(contract_address.0.key())
        .chain(&ZERO) // No entry point selector in deploy account transaction.
        .chain(&calldata_hash)
        .chain(&transaction.max_fee.0.into())
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&transaction.nonce.0)
        .get_hash())
}

fn get_deploy_transaction_hash(
    transaction: &DeployTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_deploy_transaction_hash(transaction, chain_id, false)
}

fn get_deprecated_deploy_transaction_hash(
    transaction: &DeployTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_deploy_transaction_hash(transaction, chain_id, true)
}

fn get_common_deploy_transaction_hash(
    transaction: &DeployTransaction,
    chain_id: &ChainId,
    is_deprecated: bool,
) -> Result<StarkHash, StarknetApiError> {
    let contract_address = calculate_contract_address(
        transaction.contract_address_salt,
        transaction.class_hash,
        &transaction.constructor_calldata,
        ContractAddress::from(0_u8),
    )?;

    Ok(PedersenHashChain::new()
        .chain(&DEPLOY)
        .chain_if(&transaction.version.0, !is_deprecated)
        .chain(contract_address.0.key())
        .chain(&CONSTRUCTOR_ENTRY_POINT_SELECTOR)
        .chain(
            &PedersenHashChain::new()
                .chain_iter(transaction.constructor_calldata.0.iter())
                .get_hash(),
        )
        .chain_if(&ZERO, !is_deprecated) // No fee in deploy transaction.
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .get_hash())
}

fn get_invoke_transaction_v0_hash(
    transaction: &InvokeTransactionV0,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_invoke_transaction_v0_hash(transaction, chain_id, false)
}

fn get_deprecated_invoke_transaction_v0_hash(
    transaction: &InvokeTransactionV0,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_invoke_transaction_v0_hash(transaction, chain_id, true)
}

fn get_common_invoke_transaction_v0_hash(
    transaction: &InvokeTransactionV0,
    chain_id: &ChainId,
    is_deprecated: bool,
) -> Result<StarkHash, StarknetApiError> {
    Ok(PedersenHashChain::new()
        .chain(&INVOKE)
        .chain_if(&ZERO, !is_deprecated) // Version
        .chain(transaction.contract_address.0.key())
        .chain(&transaction.entry_point_selector.0)
        .chain(&PedersenHashChain::new().chain_iter(transaction.calldata.0.iter()).get_hash())
        .chain_if(&transaction.max_fee.0.into(), !is_deprecated)
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .get_hash())
}

fn get_invoke_transaction_v1_hash(
    transaction: &InvokeTransactionV1,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    Ok(PedersenHashChain::new()
        .chain(&INVOKE)
        .chain(&ONE) // Version
        .chain(transaction.sender_address.0.key())
        .chain(&ZERO) // No entry point selector in invoke transaction.
        .chain(&PedersenHashChain::new().chain_iter(transaction.calldata.0.iter()).get_hash())
        .chain(&transaction.max_fee.0.into())
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&transaction.nonce.0)
        .get_hash())
}

fn get_l1_handler_transaction_hash(
    transaction: &L1HandlerTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_l1_handler_transaction_hash(transaction, chain_id, false)
}

fn get_deprecated_l1_handler_transaction_hash(
    transaction: &L1HandlerTransaction,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    get_common_l1_handler_transaction_hash(transaction, chain_id, true)
}

fn get_common_l1_handler_transaction_hash(
    transaction: &L1HandlerTransaction,
    chain_id: &ChainId,
    is_deprecated: bool,
) -> Result<StarkHash, StarknetApiError> {
    Ok(PedersenHashChain::new()
        .chain(&L1_HANDLER)
        .chain_if(&transaction.version.0, !is_deprecated)
        .chain(transaction.contract_address.0.key())
        .chain(&transaction.entry_point_selector.0)
        .chain(&PedersenHashChain::new().chain_iter(transaction.calldata.0.iter()).get_hash())
        .chain_if(&ZERO, !is_deprecated) // No fee in l1 handler transaction.
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&transaction.nonce.0)
        .get_hash())
}

fn get_declare_transaction_v0_hash(
    transaction: &DeclareTransactionV0V1,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    Ok(PedersenHashChain::new()
        .chain(&DECLARE)
        .chain(&ZERO) // Version
        .chain(transaction.sender_address.0.key())
        .chain(&ZERO ) // No entry point selector in declare transaction.
        .chain(&PedersenHashChain::new().get_hash())
        .chain(&transaction.max_fee.0.into())
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&transaction.class_hash.0)
        .get_hash())
}

fn get_declare_transaction_v1_hash(
    transaction: &DeclareTransactionV0V1,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    Ok(PedersenHashChain::new()
        .chain(&DECLARE)
        .chain(&ONE) // Version
        .chain(transaction.sender_address.0.key())
        .chain(&ZERO) // No entry point selector in declare transaction.
        .chain(&PedersenHashChain::new().chain(&transaction.class_hash.0).get_hash())
        .chain(&transaction.max_fee.0.into())
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&transaction.nonce.0)
        .get_hash())
}

fn get_declare_transaction_v2_hash(
    transaction: &DeclareTransactionV2,
    chain_id: &ChainId,
) -> Result<StarkHash, StarknetApiError> {
    Ok(PedersenHashChain::new()
        .chain(&DECLARE)
        .chain(&TWO) // Version
        .chain(transaction.sender_address.0.key())
        .chain(&ZERO) // No entry point selector in declare transaction.
        .chain(&PedersenHashChain::new().chain(&transaction.class_hash.0).get_hash())
        .chain(&transaction.max_fee.0.into())
        .chain(&ascii_as_felt(chain_id.0.as_str())?)
        .chain(&transaction.nonce.0)
        .chain(&transaction.compiled_class_hash.0)
        .get_hash())
}
