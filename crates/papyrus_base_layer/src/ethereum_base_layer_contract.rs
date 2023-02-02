use async_trait::async_trait;
use ethers::abi::{Abi, AbiEncode};
use ethers::contract::Contract;
use ethers::prelude::{AbiError, Address, ContractError, Http, Middleware, Provider};
use ethers::providers::ProviderError;
use ethers::types::{I256, U256};
use starknet_api::block::BlockNumber;
use starknet_api::core::GlobalRoot;
use starknet_api::hash::StarkFelt;
use starknet_api::StarknetApiError;
use url::ParseError;

use crate::BaseLayerContract;
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

#[derive(thiserror::Error, Debug)]
pub enum EthereumBaseLayerError {
    #[error(transparent)]
    FromHex(#[from] rustc_hex::FromHexError),
    #[error(transparent)]
    Abi(#[from] AbiError),
    #[error(transparent)]
    Url(#[from] ParseError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    BadContract(#[from] ContractError<Provider<Http>>),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
}

pub struct EthereumBaseLayerConfig {
    // TODO(yair): consider using types.
    pub node_url: String,
    pub starknet_contract_address: String,
}

pub struct EthereumBaseLayerContract {
    contract: Contract<Provider<Http>>,
}

impl EthereumBaseLayerContract {
    pub fn new(config: EthereumBaseLayerConfig) -> Result<Self, EthereumBaseLayerError> {
        let address = config.starknet_contract_address.parse::<Address>()?;
        let client: Provider<Http> = Provider::<Http>::try_from(config.node_url)?;
        let abi: Abi = serde_json::from_str::<Abi>(STARKNET_ABI)?;
        Ok(Self { contract: Contract::new(address, abi, client) })
    }
}

#[async_trait]
impl BaseLayerContract for EthereumBaseLayerContract {
    type Error = EthereumBaseLayerError;
    async fn latest_proved_block(
        &self,
        min_confirmations: Option<u64>,
    ) -> Result<(BlockNumber, GlobalRoot), Self::Error> {
        let ethereum_block_number =
            self.contract.client().get_block_number().await? - min_confirmations.unwrap_or(0);
        let call_state_block_number =
            self.contract.method::<_, I256>("stateBlockNumber", ())?.block(ethereum_block_number);
        let call_state_root =
            self.contract.method::<_, U256>("stateRoot", ())?.block(ethereum_block_number);
        let (state_block_number, state_root) =
            tokio::try_join!(call_state_block_number.call(), call_state_root.call())?;
        Ok((
            BlockNumber(state_block_number.as_u64()),
            GlobalRoot(StarkFelt::try_from(state_root.encode_hex().as_str())?),
        ))
    }
}
