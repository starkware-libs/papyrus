use std::fs::File;
use std::process::Command;

use assert_matches::assert_matches;
use ethers::utils::{Ganache, GanacheInstance};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
use tar::Archive;
use tempfile::{tempdir, TempDir};

use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::BaseLayerContract;

// Returns a Ganache instance, preset with a Starknet core contract and some state updates:
//     starknet block number   starknet block hash   Ethereum block number
//     100                     0x100                  10
//     200                     0x200                  20
//     300                     0x300                  30
// The blockchain is at Ethereum block number 31.
// Note: Requires Ganache@7.4.3 installed.
fn get_test_ethereum_node() -> (GanacheInstance, TempDir) {
    // Verify correct Ganache version.
    let ganache_version = String::from_utf8_lossy(
        &Command::new("ganache")
            .arg("--version")
            .output()
            .expect("Failed to get Ganache version, check if it is installed.")
            .stdout,
    )
    .to_string();
    assert!(
        ganache_version.starts_with("ganache v7.4.3"),
        "Wrong Ganache version, please install v7.4.3"
    );
    const GANACHE_DB_TAR_PATH: &str = "resources/ganache-db.tar";
    const REL_DB_PATH: &str = "hathat";

    // Unpack the Ganache db tar file into a temporary dir.
    let mut ar = Archive::new(File::open(GANACHE_DB_TAR_PATH).unwrap());
    let ganache_db = tempdir().unwrap();
    ar.unpack(ganache_db.path()).unwrap();

    // Start Ganache instance. This will panic if Ganache is not installed.
    let db_path = ganache_db.path().join(REL_DB_PATH);
    let ganache = Ganache::new().args(["--db", db_path.to_str().unwrap()]).spawn();

    (ganache, ganache_db)
}

#[test_with::executable(ganache)]
#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    const SN_CONTRACT_ADDR: &str = "0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A";
    let (node, _db) = get_test_ethereum_node();

    let config = EthereumBaseLayerConfig {
        node_url: node.endpoint(),
        starknet_contract_address: SN_CONTRACT_ADDR.to_owned(),
    };
    let contract = EthereumBaseLayerContract::new(config).unwrap();

    let latest_state_0_conf = contract.latest_proved_block(None).await;
    assert_matches!(latest_state_0_conf, Ok(Some((block_number, block_hash)))
        if block_number == BlockNumber(300) && block_hash == BlockHash(stark_felt!("0x300")));

    let latest_state_5_conf = contract.latest_proved_block(Some(5)).await;
    assert_matches!(latest_state_5_conf, Ok(Some((block_number, block_hash)))
        if block_number == BlockNumber(300) && block_hash == BlockHash(stark_felt!("0x300")));

    let latest_state_15_conf = contract.latest_proved_block(Some(15)).await;
    assert_matches!(latest_state_15_conf, Ok(Some((block_number, block_hash)))
        if block_number == BlockNumber(200) && block_hash == BlockHash(stark_felt!("0x200")));

    let latest_state_25_conf = contract.latest_proved_block(Some(25)).await;
    assert_matches!(latest_state_25_conf, Ok(Some((block_number, block_hash)))
        if block_number == BlockNumber(100) && block_hash == BlockHash(stark_felt!("0x100")));

    let latest_state_1000_conf = contract.latest_proved_block(Some(1000)).await;
    assert_matches!(latest_state_1000_conf, Ok(None));
}
