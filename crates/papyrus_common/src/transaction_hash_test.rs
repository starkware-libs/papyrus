use std::sync::Arc;

use starknet_api::core::{ChainId, ClassHash, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    DeployAccountTransaction,
    Fee,
    TransactionSignature,
    TransactionVersion,
};

use super::{ascii_as_felt, get_deploy_account_transaction_hash};

#[test]
fn test_ascii_as_felt() {
    let sn_main_id = ChainId("SN_MAIN".to_owned());
    let sn_main_felt = ascii_as_felt(sn_main_id.0.as_str()).unwrap();
    let expected_sn_main = StarkFelt::from(23448594291968334_u128);
    assert_eq!(sn_main_felt, expected_sn_main);
}

#[test]
fn test_tx_type_hash() {
    let deploy_account_felt = ascii_as_felt("deploy_account").unwrap();
    let expected_deploy_account = StarkFelt::from(2036277798190617858034555652763252_u128);
    assert_eq!(deploy_account_felt, expected_deploy_account);
}

fn hex_as_felt(hex_str: &str) -> StarkFelt {
    StarkFelt::try_from(hex_str).unwrap()
}

#[test]
fn test_deploy_account_transaction_hash() {
    let tx = DeployAccountTransaction {
        max_fee: Fee(u128::from_str_radix("81ca6d7a5d", 16).unwrap()),
        version: TransactionVersion(hex_as_felt("0x1")),
        signature: TransactionSignature(vec![
            hex_as_felt("0x787e77d333665c7c830a8257e36270410f038fb278e8a468d5c89ec09f2c361"),
            hex_as_felt("0x4310e1aad5c88a32439bf5b64b0b4114c2a42c8468aa97f374a0738d7733a49"),
        ]),
        nonce: Nonce(hex_as_felt("0x0")),
        class_hash: ClassHash(hex_as_felt(
            "0x1fac3074c9d5282f0acc5c69a4781a1c711efea5e73c550c5d9fb253cf7fd3d",
        )),
        contract_address_salt: ContractAddressSalt(hex_as_felt(
            "0x548987f6a88ad1a506c639f4da4571dfabcfa7fa2abc5da30d3c3c1af71b5d9",
        )),
        constructor_calldata: Calldata(Arc::new(vec![hex_as_felt(
            "0x3d03f1e51b6be7edf29c4fa772cf4294259a8449ed32e76ee2169479b06afdd",
        )])),
    };
    let chain_id = ChainId("SN_GOERLI".to_owned());
    let actual_tx_hash = get_deploy_account_transaction_hash(&tx, &chain_id).unwrap();
    let expected_tx_hash =
        hex_as_felt("0x00d74a7310e8739a8586b851b928bcc32c955009e3a49057e986d0f2c0a06f16");
    assert_eq!(actual_tx_hash, expected_tx_hash);
}
