use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};
use starknet_api::core::ChainId;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::Transaction;
use test_utils::read_json_file;

use super::{
    ascii_as_felt,
    get_transaction_hash,
    validate_transaction_hash,
    CONSTRUCTOR_ENTRY_POINT_SELECTOR,
};

#[test]
fn test_ascii_as_felt() {
    let sn_main_id = ChainId("SN_MAIN".to_owned());
    let sn_main_felt = ascii_as_felt(sn_main_id.0.as_str()).unwrap();
    // This is the result of the Python snippet from the Chain-Id documentation.
    let expected_sn_main = StarkFelt::from(23448594291968334_u128);
    assert_eq!(sn_main_felt, expected_sn_main);
}

#[test]
fn test_constructor_selector() {
    let mut keccak = Keccak256::default();
    keccak.update(b"constructor");
    let mut constructor_bytes: [u8; 32] = keccak.finalize().into();
    constructor_bytes[0] &= 0b00000011_u8; // Discard the six MSBs.
    let constructor_felt = StarkFelt::new(constructor_bytes).unwrap();
    assert_eq!(constructor_felt, *CONSTRUCTOR_ENTRY_POINT_SELECTOR);
}

#[derive(Deserialize, Serialize)]
struct TransactionWithHash {
    transaction: Transaction,
    transaction_hash: StarkHash,
}

#[test]
fn test_transaction_hash() {
    // The details were taken from Starknet Goerli. You can found the transactions by hash in:
    // https://alpha4.starknet.io/feeder_gateway/get_transaction?transactionHash=<transaction_hash>
    let chain_id = ChainId("SN_GOERLI".to_owned());
    let transactions_with_hash: Vec<TransactionWithHash> =
        serde_json::from_value(read_json_file("transaction_hash.json")).unwrap();

    for transaction_with_hash in transactions_with_hash {
        assert!(
            validate_transaction_hash(
                &transaction_with_hash.transaction,
                &chain_id,
                transaction_with_hash.transaction_hash
            )
            .unwrap()
        );
        let actual_transaction_hash =
            get_transaction_hash(&transaction_with_hash.transaction, &chain_id).unwrap();
        assert_eq!(actual_transaction_hash, transaction_with_hash.transaction_hash);
    }
}

#[test]
fn test_deprecated_transaction_hash() {
    // The details were taken from Starknet Goerli. You can found the transactions by hash in:
    // https://alpha4.starknet.io/feeder_gateway/get_transaction?transactionHash=<transaction_hash>
    let chain_id = ChainId("SN_GOERLI".to_owned());
    let transactions_with_hash: Vec<TransactionWithHash> =
        serde_json::from_value(read_json_file("deprecated_transaction_hash.json")).unwrap();

    for transaction_with_hash in transactions_with_hash {
        assert!(
            validate_transaction_hash(
                &transaction_with_hash.transaction,
                &chain_id,
                transaction_with_hash.transaction_hash
            )
            .unwrap()
        );
    }
}

#[tokio::test]
async fn test_all_blocks() {
    for block_number in 192..193 {
        validate_block(block_number).await;
    }
}

async fn validate_block(block_number: usize) {
    let chain_id = ChainId("SN_MAIN".to_owned());
    let url = "http://papyrus-mainnet-bm.starknet.io/rpc/v0_4";
    let method = "starknet_getBlockWithTxs";
    // let params = r#"{"block_number": 0}"#;
    let params = format!("{{\"block_number\": {}}}", block_number);

    let client = reqwest::Client::new();
    let res = client
    .post(url)
    .header("Content-Type", "application/json")
    .body(format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method}","params":[{params}]}}"#))
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();


    let block: Value = serde_json::from_str(&res).unwrap();
    let block = block.as_object().unwrap().get("result").unwrap();
    let json_txs = block.get("transactions").unwrap().as_array().unwrap();
    for json_tx in json_txs {
        let mut json_obj_tx = json_tx.as_object().unwrap().to_owned();
        let tx_hash = json_obj_tx.remove("transaction_hash").unwrap();
        let expected_hash = StarkFelt::try_from(tx_hash.as_str().unwrap()).unwrap();

        let tx_type = json_obj_tx.remove("type").unwrap();
        let tx_type = if tx_type == "L1_HANDLER" {
            "L1Handler".to_owned()
        } else {
            let temp_tx_type = tx_type.as_str().unwrap().to_owned();
            format!("{}{}", temp_tx_type.chars().next().unwrap(), temp_tx_type[1..].to_lowercase())
        };

        let version = {
            if tx_type == "Declare" || tx_type == "Invoke" {
                json_obj_tx.remove("version")
            } else {
                None
            }
        };

        let mut tx = json!({});
        let tx_content = match version {
            Some(ver) => {
                let ver_str = format!("V{}", ver.as_str().unwrap().strip_prefix("0x").unwrap());
                let mut tx_content_val = json!({});
                tx_content_val[ver_str] = json!(json_obj_tx);
                tx_content_val
            },
            None => json!(json_obj_tx),
        };
        tx[tx_type] = tx_content;
        let tx_obj: Transaction = serde_json::from_value(tx).expect("serde_json error");


        let is_valid = validate_transaction_hash(&tx_obj, &chain_id, expected_hash).unwrap();
        assert!(is_valid, "Invalid tx hash {}", expected_hash);
    }
}
