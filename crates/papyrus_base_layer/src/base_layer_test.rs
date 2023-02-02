use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::BaseLayerContract;

const SN_CONTRACT_ADDR_MAINNET: &str = "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4";
const INFURA_ADDR: &str = "https://mainnet.infura.io/v3/c60b0bb42f8a4c6481ecd229eddaca27";

#[tokio::test]
async fn use_trait() {
    let config = EthereumBaseLayerConfig {
        node_url: INFURA_ADDR.to_owned(),
        starknet_contract_address: SN_CONTRACT_ADDR_MAINNET.to_owned(),
    };
    let base = EthereumBaseLayerContract::new(config).unwrap();
    let (_bn, _root) = base.latest_proved_block(Some(3)).await.unwrap();
}
