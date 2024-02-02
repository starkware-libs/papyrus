use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use ethers::abi::{Abi, AbiEncode};
use ethers::contract::Contract;
use ethers::prelude::{AbiError, Address, ContractError, Http, Middleware, Provider};
use ethers::providers::ProviderError;
use ethers::types::{I256, U256};
use papyrus_config::dumping::{ser_param, ser_required_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;
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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct EthereumBaseLayerConfig {
    // TODO(yair): consider using types.
    pub node_url: String,
    pub starknet_contract_address: String,
}

impl SerializeConfig for EthereumBaseLayerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_required_param(
                "node_url",
                SerializationType::String,
                "Ethereum node URL. A schema to match to Infura node: https://mainnet.infura.io/v3/<your_api_key>, but any other node can be used.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "starknet_contract_address",
                &self.starknet_contract_address,
                "Starknet contract address in ethereum.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for EthereumBaseLayerConfig {
    fn default() -> Self {
        Self {
            node_url: "https://mainnet.infura.io/v3/<your_api_key>".to_string(),
            starknet_contract_address: "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4".to_string(),
        }
    }
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
        Ok(Self { contract: Contract::new(address, abi, Arc::new(client)) })
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
        let Some(ethereum_block_number) = ethereum_block_number else {
            return Ok(None);
        };

        let call_state_block_number =
            self.contract.method::<_, I256>("stateBlockNumber", ())?.block(ethereum_block_number);
        let call_state_block_hash =
            self.contract.method::<_, U256>("stateBlockHash", ())?.block(ethereum_block_number);
        let (state_block_number, state_block_hash) =
            tokio::try_join!(call_state_block_number.call(), call_state_block_hash.call())?;

        Ok(Some((
            BlockNumber(state_block_number.as_u64()),
            BlockHash(
                Felt::from_hex(state_block_hash.encode_hex().as_str())
                    .expect("Invalid starknet block hash"),
            ),
        )))
    }
}
