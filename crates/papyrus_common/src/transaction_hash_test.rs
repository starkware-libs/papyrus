use std::io::BufReader;

use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::DeployAccountTransaction;
use test_utils::get_absolute_path;

use super::{ascii_as_felt, get_deploy_account_transaction_hash};

#[test]
fn test_ascii_as_felt() {
    let sn_main_id = ChainId("SN_MAIN".to_owned());
    let sn_main_felt = ascii_as_felt(sn_main_id.0.as_str()).unwrap();
    // This is the result of the Python snippet from the Chain-Id documentation.
    let expected_sn_main = StarkFelt::from(23448594291968334_u128);
    assert_eq!(sn_main_felt, expected_sn_main);
}

#[derive(Deserialize, Serialize)]
struct TxWithHash {
    tx: DeployAccountTransaction,
    tx_hash: StarkHash,
}

#[test]
fn test_deploy_account_transaction_hash() {
    let file_path = get_absolute_path("crates/papyrus_common/resources/tx_hash.json");
    let file = std::fs::File::open(file_path).unwrap();
    let reader = BufReader::new(file);
    let tx_with_hash: TxWithHash = serde_json::from_reader(reader).unwrap();

    let chain_id = ChainId("SN_GOERLI".to_owned());
    let actual_tx_hash = get_deploy_account_transaction_hash(&tx_with_hash.tx, &chain_id).unwrap();
    assert_eq!(actual_tx_hash, tx_with_hash.tx_hash);
}
