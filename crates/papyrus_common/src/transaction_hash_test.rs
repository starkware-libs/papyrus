use std::io::BufReader;

use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::Transaction;
use test_utils::get_absolute_path;

use super::{ascii_as_felt, get_tx_hash, validate_tx_hash};

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
    tx: Transaction,
    tx_hash: StarkHash,
    is_deprecated: bool,
}

#[test]
fn test_transaction_hash() {
    // The tx details were taken from
    // https://alpha4.starknet.io/feeder_gateway/get_block?blockNumber=385429
    // https://alpha4.starknet.io/feeder_gateway/get_block?blockNumber=0
    let chain_id = ChainId("SN_GOERLI".to_owned());

    let file_path = get_absolute_path("crates/papyrus_common/resources/tx_hash.json");
    let file = std::fs::File::open(file_path).unwrap();
    let reader = BufReader::new(file);
    let txs_with_hash: Vec<TxWithHash> = serde_json::from_reader(reader).unwrap();

    for tx_with_hash in txs_with_hash {
        assert!(validate_tx_hash(&tx_with_hash.tx, &chain_id, tx_with_hash.tx_hash).unwrap());
        if !tx_with_hash.is_deprecated {
            let actual_tx_hash = get_tx_hash(&tx_with_hash.tx, &chain_id).unwrap();
            assert_eq!(actual_tx_hash, tx_with_hash.tx_hash);
        }
    }
}
