use std::fs::File;
use std::process::Command;

use ethers::utils::{Ganache, GanacheInstance};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
use tar::Archive;
use tempfile::{tempdir, TempDir};

use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::BaseLayerContract;

type EthereumContractAddress = String;

// Returns a Ganache instance, preset with a Starknet core contract and some state updates:
// Starknet contract address: 0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A
//     Ethereum block number   starknet block number   starknet block hash
//      10                      100                     0x100
//      20                      200                     0x200
//      30                      300                     0x300
// The blockchain is at Ethereum block number 31.
// Note: Requires Ganache@7.4.3 installed.
fn get_test_ethereum_node() -> (GanacheInstance, TempDir, EthereumContractAddress) {
    const SN_CONTRACT_ADDR: &str = "0xe2aF2c1AE11fE13aFDb7598D0836398108a4db0A";
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

    (ganache, ganache_db, SN_CONTRACT_ADDR.to_owned())
}

#[test_with::executable(ganache)]
#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    let (node, _db, starknet_contract_address) = get_test_ethereum_node();
    let config = EthereumBaseLayerConfig { node_url: node.endpoint(), starknet_contract_address };
    let contract = EthereumBaseLayerContract::new(config).unwrap();

    type Scenario = (Option<u64>, Option<(BlockNumber, BlockHash)>);
    let scenarios: Vec<Scenario> = vec![
        (None, Some((BlockNumber(300), BlockHash(stark_felt!("0x300"))))),
        (Some(5), Some((BlockNumber(300), BlockHash(stark_felt!("0x300"))))),
        (Some(15), Some((BlockNumber(200), BlockHash(stark_felt!("0x200"))))),
        (Some(25), Some((BlockNumber(100), BlockHash(stark_felt!("0x100"))))),
        (Some(1000), None),
    ];
    for (scenario, expected) in scenarios {
        let latest_block = contract.latest_proved_block(scenario).await.unwrap();
        assert_eq!(latest_block, expected);
    }
}
