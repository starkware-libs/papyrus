use std::collections::BTreeMap;
use std::ops::AddAssign;
use std::time::Duration;

use anyhow::Ok;
use derive_more::AddAssign;
use lazy_static::lazy_static;
use papyrus_common::block_hash::{fill_missing_header_fields, BlockHashError, BlockHashVersion};
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_param,
    ser_pointer_target_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_node::version::VERSION_FULL;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageConfig, StorageReader};
use papyrus_sync::sources::central::CentralSourceConfig;
use papyrus_sync::verification::{VerificationError, VerificationResult, Verifier, VerifierImpl};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockBody, BlockHeader, BlockNumber, BlockVerificationError};
use starknet_api::core::{ChainId, SequencerPublicKey, StateDiffCommitment};
use starknet_api::hash::PoseidonHash;
use starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};
use tracing::info;
use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

// Mimicking the config structure of the node so the env variables will be the same.
lazy_static! {
    static ref CONFIG_POINTERS: Vec<((ParamPath, SerializedParam), Vec<ParamPath>)> = vec![(
        ser_param(
            "chain_id",
            &ChainId("SN_MAIN".to_string()),
            "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
            ParamPrivacyInput::Public,
        ),
        vec!["storage.db_config.chain_id".to_owned()],
    ),
    (
        ser_pointer_target_param(
            "starknet_url",
            &"https://alpha-mainnet.starknet.io/".to_string(),
            "The URL of a centralized Starknet gateway.",
        ),
        vec!["central.url".to_owned()],
    )];
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct VerifyStorageConfig {
    pub storage: StorageConfig,
    pub central: CentralSourceConfig,
    // Hack - this is both a pointer target and a param.
    // Skip dumping it as it will be dumped when the pointers are resolved.
    pub chain_id: ChainId,
}

impl Default for VerifyStorageConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            central: CentralSourceConfig::default(),
            chain_id: ChainId("SN_MAIN".to_string()),
        }
    }
}

impl SerializeConfig for VerifyStorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        itertools::chain!(
            append_sub_config_name(self.central.dump(), "central"),
            append_sub_config_name(self.storage.dump(), "storage"),
        )
        .collect()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default, AddAssign)]
struct SignatureStatistics {
    pub total_signature_read_time: Duration,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
struct HeaderStatistics {
    pub v0_first_block: Option<BlockNumber>,
    pub v1_first_block: Option<BlockNumber>,
    pub v2_first_block: Option<BlockNumber>,
    pub v3_first_block: Option<BlockNumber>,
    pub first_header_with_commitments: Option<BlockNumber>,
    pub total_header_fixing_time: Duration,
}

impl AddAssign for HeaderStatistics {
    fn add_assign(&mut self, other: Self) {
        self.total_header_fixing_time += other.total_header_fixing_time;
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default, AddAssign)]
struct Statistics {
    pub n_blocks: usize,
    pub total_verification_time: Duration,
    pub total_storage_read_time: Duration,
    pub total_signature_verification_time: Duration,
    pub total_header_verification_time: Duration,
    pub total_body_verification_time: Duration,
    pub signature_statistics: SignatureStatistics,
    pub header_statistics: HeaderStatistics,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let temp_file = tempfile::NamedTempFile::new()?;
    VerifyStorageConfig::default()
        .dump_to_file(&CONFIG_POINTERS, temp_file.path().to_str().expect("temp file path error"))?;

    let command = clap::Command::new("StorageVerifier")
        .about("Tool to verify the storage of a Papyrus node.");
    let config = load_and_process_config::<VerifyStorageConfig>(
        temp_file.into_file(),
        command,
        std::env::args().collect(),
    );
    if let Err(papyrus_config::ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }
    let config = config?;
    configure_tracing();
    info!("Starting verification with config: {config:#?}");

    let (storage_reader, _) = papyrus_storage::open_storage(config.storage)?;
    info!("Opened storage.");
    let starknet_client = StarknetFeederGatewayClient::new(
        &config.central.url,
        config.central.http_headers,
        VERSION_FULL,
        config.central.retry_config,
    )?;
    info!("Initialized Starknet client.");
    let sequencer_pub_key = starknet_client.sequencer_pub_key().await?;
    info!("Got sequencer public key.");
    let mut statistics = Statistics::default();
    info!("Starting verification.");
    let start = std::time::Instant::now();

    let latest_block = storage_reader.begin_ro_txn()?.get_state_marker()?;
    for bn in BlockNumber(0).iter_up_to(latest_block) {
        statistics.n_blocks += 1;
        if bn.0 % 1000 == 0 {
            info!("Got to block {bn}. {statistics:#?}");
        }

        let start = std::time::Instant::now();
        let header = storage_reader.begin_ro_txn()?.get_block_header(bn)?.unwrap_or_else(|| {
            panic!("Header for block number {} is missing.", bn);
        });
        statistics.total_storage_read_time += start.elapsed();

        let start = std::time::Instant::now();
        verify_signature(
            bn,
            &header,
            storage_reader.clone(),
            &sequencer_pub_key,
            &mut statistics.signature_statistics,
        )?;
        statistics.total_signature_verification_time += start.elapsed();

        let start = std::time::Instant::now();
        validate_header(
            bn,
            &header,
            &config.chain_id,
            &mut statistics.header_statistics,
            &starknet_client,
        )
        .await?;
        statistics.total_header_verification_time += start.elapsed();
    }
    statistics.total_verification_time = start.elapsed();

    println!("Statistics: {statistics:#?}");
    Ok(())
}

fn configure_tracing() {
    let fmt_layer = tracing_subscriber::fmt::layer().compact().with_target(false);
    let level_filter_layer = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(DEFAULT_LEVEL.into())
        .from_env_lossy();

    // This sets a single subscriber to all of the threads. We may want to implement different
    // subscriber for some threads and use set_global_default instead of init.
    tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
}

fn verify_signature(
    bn: BlockNumber,
    header: &BlockHeader,
    storage_reader: StorageReader,
    sequencer_pub_key: &SequencerPublicKey,
    statistics: &mut SignatureStatistics,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let signature = storage_reader.begin_ro_txn()?.get_block_signature(bn)?.unwrap_or_else(|| {
        panic!("Signature for block number {} is missing.", bn);
    });
    statistics.total_signature_read_time += start.elapsed();

    let state_diff_commitment =
        header.state_diff_commitment.expect("Missing state diff commitment");

    if !VerifierImpl::verify_signature(
        &header.block_hash,
        &state_diff_commitment,
        &signature,
        sequencer_pub_key,
    )? {
        panic!("Failed to verify signature for block number {}.", bn);
    }

    Ok(())
}

async fn validate_header(
    bn: BlockNumber,
    header: &BlockHeader,
    chain_id: &ChainId,
    statistics: &mut HeaderStatistics,
    starknet_client: &StarknetFeederGatewayClient,
) -> anyhow::Result<()> {
    let block_hash_version = if statistics.v3_first_block.is_some() {
        BlockHashVersion::V3
    } else if statistics.v2_first_block.is_some() {
        BlockHashVersion::V2
    } else if statistics.v1_first_block.is_some() {
        BlockHashVersion::V1
    } else {
        BlockHashVersion::V0
    };

    if statistics.first_header_with_commitments.is_none() {
        let start = std::time::Instant::now();
        if header.transaction_commitment.is_some() && header.event_commitment.is_some() {
            statistics.first_header_with_commitments = Some(bn);
        } else {
            let client_block =
                starknet_client.latest_block().await?.expect("Latest block should exist.");
            let signature_data = starknet_client
                .block_signature(client_block.block_number())
                .await?
                .expect("Latest block signature should exist.");

            let mut block = client_block
                .to_starknet_api_block_and_version(
                    signature_data.signature_input.state_diff_commitment,
                )
                .unwrap();

            fill_missing_header_fields(&mut block, block_hash_version);
            if !VerifierImpl::validate_header(&block.header, chain_id, block_hash_version)? {
                panic!("Failed to validate header for block number {}.", bn);
            }
        }
        statistics.total_header_fixing_time += start.elapsed();
    }

    if !VerifierImpl::validate_header(header, chain_id, block_hash_version)? {
        panic!("Failed to validate header for block number {}.", bn);
    }
    Ok(())
}
