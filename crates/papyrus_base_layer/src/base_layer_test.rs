use std::convert::TryFrom;

use ethers::abi::{Abi, AbiEncode};
use ethers::contract::Contract;
use ethers::prelude::*;
use starknet_api::block::BlockNumber;
use starknet_api::core::GlobalRoot;
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
const SN_CONTRACT_ADDR_MAINNET: &str = "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4";
const INFURA_ADDR: &str = "https://mainnet.infura.io/v3/c60b0bb42f8a4c6481ecd229eddaca27";

const STARKNET_ABI: &str = r#"[
  {
    "inputs": [

    ],
    "name": "stateBlockNumber",
    "outputs": [
      {
        "internalType": "int256",
        "name": "",
        "type": "int256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [

    ],
    "name": "stateRoot",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  }
]
"#;

fn get_test_provider() -> Provider<Http> {
    Provider::<Http>::try_from(INFURA_ADDR).expect("could not instantiate HTTP Provider")
}

fn get_starknet_contract() -> Contract<Provider<Http>> {
    let provider = get_test_provider();
    let addr = SN_CONTRACT_ADDR_MAINNET.parse::<Address>().unwrap();
    let abi = serde_json::from_str::<Abi>(STARKNET_ABI).unwrap();
    Contract::new(addr, abi, provider)
}

#[tokio::test]
async fn base_layer_state_block_number() {
    let contract = get_starknet_contract();
    let bn = contract.method::<_, I256>("stateBlockNumber", ()).unwrap().await.unwrap();

    assert!(I256::is_positive(bn));
}

#[tokio::test]
async fn base_layer_state_root() {
    let contract = get_starknet_contract();
    let state_root = contract.method::<_, U256>("stateRoot", ()).unwrap().await.unwrap();

    assert!(!state_root.is_zero());
}

#[tokio::test]
async fn data_at_block() {
    let contract = get_starknet_contract();
    let eth_block_number = U64::from(16576509);

    let state_block_number = BlockNumber(
        contract
            .method::<_, I256>("stateBlockNumber", ())
            .unwrap()
            .block(eth_block_number)
            .await
            .unwrap()
            .as_u64(),
    );

    assert_eq!(state_block_number, BlockNumber(19883));
    let state_root = GlobalRoot(
        StarkFelt::try_from(
            contract
                .method::<_, U256>("stateRoot", ())
                .unwrap()
                .block(eth_block_number)
                .await
                .unwrap()
                .encode_hex()
                .as_str(),
        )
        .unwrap(),
    );
    assert_eq!(
        state_root,
        GlobalRoot(stark_felt!(
            "0x0052d5a46c28465f849e08c510be9024ac484aa4383bbb0745a9da5ef6a2d631"
        ))
    );
}
