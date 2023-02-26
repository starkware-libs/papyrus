use async_trait::async_trait;
use ethers::abi::{Abi, AbiEncode};
use ethers::contract::Contract;
use ethers::prelude::{AbiError, Address, ContractError, Http, Middleware, Provider};
use ethers::providers::ProviderError;
use ethers::types::{I256, U256};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkHash;
use starknet_api::StarknetApiError;
use url::ParseError;

use crate::BaseLayerContract;

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
        // The solidity contract was pre-compiled, and only the relevant functions were kept.
        let abi: Abi = serde_json::from_str::<Abi>(include_str!("core_contract_latest_block.abi"))?;
        Ok(Self { contract: Contract::new(address, abi, client) })
    }
}

#[async_trait]
impl BaseLayerContract for EthereumBaseLayerContract {
    type Error = EthereumBaseLayerError;

    async fn latest_proved_block(
        &self,
        min_confirmations: Option<u64>,
    ) -> Result<Option<(BlockNumber, BlockHash)>, Self::Error> {
        let ethereum_block_number = self
            .contract
            .client()
            .get_block_number()
            .await?
            .checked_sub(min_confirmations.unwrap_or(0).into());
        if ethereum_block_number.is_none() {
            return Ok(None);
        }
        let ethereum_block_number = ethereum_block_number.unwrap();

        let call_state_block_number =
            self.contract.method::<_, I256>("stateBlockNumber", ())?.block(ethereum_block_number);
        let call_state_block_hash =
            self.contract.method::<_, U256>("stateBlockHash", ())?.block(ethereum_block_number);
        let (state_block_number, state_block_hash) =
            tokio::try_join!(call_state_block_number.call(), call_state_block_hash.call())?;

        Ok(Some((
            BlockNumber(state_block_number.as_u64()),
            BlockHash(StarkHash::try_from(state_block_hash.encode_hex().as_str())?),
        )))
    }
}
