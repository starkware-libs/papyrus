use std::collections::BTreeMap;
use std::iter::zip;
use std::ops::AddAssign;
use std::time::Duration;

use anyhow::Ok;
use derive_more::AddAssign;
use itertools::enumerate;
use lazy_static::lazy_static;
use papyrus_common::block_hash::{
    calculate_event_commitment_by_version,
    calculate_transaction_commitment_by_version,
};
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_param,
    ser_pointer_target_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_node::version::VERSION_FULL;
use papyrus_storage::body::events::{EventIndex, EventsReader};
use papyrus_storage::body::{BodyStorageReader, TransactionIndex};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageConfig, StorageReader};
use papyrus_sync::sources::central::CentralSourceConfig;
use papyrus_sync::verification::{get_block_hash_version, CentralSourceVerifier, Verifier};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::{ChainId, SequencerPublicKey};
use starknet_api::state::StateNumber;
use starknet_api::transaction::{Event, EventIndexInTransactionOutput, TransactionOffsetInBlock};
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
struct Statistics {
    pub n_blocks: usize,
    pub total_verification_time: Duration,
    pub total_storage_read_time: Duration,
    pub total_signature_verification_time: Duration,
    pub total_header_verification_time: Duration,
    pub total_body_verification_time: Duration,
    pub signature_statistics: SignatureStatistics,
    pub total_header_fixing_time: Duration,
    pub total_transactions_verification_time: Duration,
    pub total_state_diff_verification_time: Duration,
    pub total_class_verification_time: Duration,
    pub n_classes: usize,
    pub total_deprecated_class_verification_time: Duration,
    pub n_deprecated_classes: usize,
}

impl AddAssign for Statistics {
    fn add_assign(&mut self, other: Self) {
        self.total_verification_time += other.total_verification_time;
        self.total_storage_read_time += other.total_storage_read_time;
        self.total_signature_verification_time += other.total_signature_verification_time;
        self.total_header_verification_time += other.total_header_verification_time;
        self.total_body_verification_time += other.total_body_verification_time;
        self.signature_statistics += other.signature_statistics;
        self.total_header_fixing_time += other.total_header_fixing_time;
    }
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
    let mut next_update = 10;
    let latest_block = storage_reader.begin_ro_txn()?.get_state_marker()?;
    for bn in BlockNumber(0).iter_up_to(latest_block) {
        let last_update = start.elapsed().as_secs();
        if last_update >= next_update {
            info!("Got to block {bn}. {statistics:#?}");
            next_update += 10;
        }
        statistics.n_blocks += 1;

        let start = std::time::Instant::now();
        let mut header =
            storage_reader.begin_ro_txn()?.get_block_header(bn)?.unwrap_or_else(|| {
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
        if header.transaction_commitment.is_none() && header.event_commitment.is_none() {
            fix_header(&mut header, &starknet_client, &config.chain_id).await?;
        }
        statistics.total_header_fixing_time += start.elapsed();

        let start = std::time::Instant::now();
        if !CentralSourceVerifier::validate_header(&header, &config.chain_id)? {
            println!("Statistics: {statistics:#?}");
            panic!("Failed to validate header for block number {}. \n{:#?}", bn, header);
        }
        statistics.total_header_verification_time += start.elapsed();

        let start = std::time::Instant::now();
        let transactions = storage_reader
            .begin_ro_txn()?
            .get_block_transactions(bn)?
            .expect("Transactions should exist.");
        let events = storage_reader
            .begin_ro_txn()?
            .iter_events(
                None,
                EventIndex(
                    TransactionIndex(bn, TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(0),
                ),
                bn,
            )?
            .map(|((from_address, ..), content)| Event { from_address, content })
            .collect::<Vec<_>>();
        let transaction_hashes = storage_reader
            .begin_ro_txn()?
            .get_block_transaction_hashes(bn)?
            .expect("Transaction hashes should exist.");
        statistics.total_storage_read_time += start.elapsed();

        let start = std::time::Instant::now();
        if !CentralSourceVerifier::validate_body(
            &bn,
            &config.chain_id,
            &transactions,
            events.iter(),
            &transaction_hashes,
            &header.transaction_commitment.unwrap(),
            &header.event_commitment.unwrap(),
        )? {
            println!("Statistics: {statistics:#?}");
            panic!("Failed to validate body for block number {}.", bn);
        }
        statistics.total_body_verification_time += start.elapsed();

        let start = std::time::Instant::now();
        for (i, (tx, tx_hash)) in enumerate(zip(transactions, transaction_hashes)) {
            if !CentralSourceVerifier::validate_transaction(&tx, &bn, &config.chain_id, &tx_hash)? {
                println!("Statistics: {statistics:#?}");
                panic!(
                    "Failed to validate transaction {i} with expected hash {tx_hash} in block \
                     number {bn}."
                );
            }
        }
        statistics.total_transactions_verification_time += start.elapsed();

        let start = std::time::Instant::now();
        let state_diff =
            storage_reader.begin_ro_txn()?.get_state_diff(bn)?.expect("State diff should exist.");
        statistics.total_storage_read_time += start.elapsed();

        let start = std::time::Instant::now();
        if !CentralSourceVerifier::validate_state_diff(
            &state_diff,
            &header.state_diff_commitment.unwrap(),
        )? {
            println!("Statistics: {statistics:#?}");
            panic!("Failed to validate state diff for block number {}.", bn);
        }
        statistics.total_state_diff_verification_time += start.elapsed();

        for (class_hash, _compiled_class_hash) in &state_diff.declared_classes {
            let start_read = std::time::Instant::now();
            let class = storage_reader
                .begin_ro_txn()?
                .get_state_reader()?
                .get_class_definition_at(StateNumber::unchecked_right_after_block(bn), class_hash)?
                .expect("Class should exist.");
            statistics.total_storage_read_time += start_read.elapsed();
            let start = std::time::Instant::now();
            if !CentralSourceVerifier::validate_class(&class, class_hash)? {
                println!("Statistics: {statistics:#?}");
                panic!("Failed to validate class {class_hash}.");
            }
            statistics.total_class_verification_time += start.elapsed();
        }
        statistics.n_classes += state_diff.declared_classes.len();

        // Skipping the implicitly declared classes for now.
        for class_hash in &state_diff.deprecated_declared_classes {
            let start_read = std::time::Instant::now();
            let class = storage_reader
                .begin_ro_txn()?
                .get_state_reader()?
                .get_deprecated_class_definition_at(
                    StateNumber::unchecked_right_after_block(bn),
                    class_hash,
                )?
                .expect("Class should exist.");
            statistics.total_storage_read_time += start_read.elapsed();
            let start = std::time::Instant::now();
            if !CentralSourceVerifier::validate_deprecated_class(class, class_hash)? {
                println!("Statistics: {statistics:#?}");
                panic!("Failed to validate deprecated class {class_hash}.");
            }
            statistics.total_deprecated_class_verification_time += start.elapsed();
        }
        statistics.n_deprecated_classes += state_diff.deprecated_declared_classes.len();
    }
    statistics.total_verification_time = start.elapsed();

    println!("Done!! {statistics:#?}");
    Ok(())
}

async fn fix_header(
    header: &mut BlockHeader,
    starknet_client: &StarknetFeederGatewayClient,
    chain_id: &ChainId,
) -> Result<(), anyhow::Error> {
    let client_block =
        starknet_client.block(header.block_number).await?.expect("Latest block should exist.");
    let signature_data = starknet_client
        .block_signature(client_block.block_number())
        .await?
        .expect("Latest block signature should exist.");

    let block = client_block
        .to_starknet_api_block_and_version(signature_data.signature_input.state_diff_commitment)
        .unwrap();

    header.transaction_commitment.get_or_insert_with(|| {
        let block_hash_version = get_block_hash_version(chain_id, &header.block_number);
        calculate_transaction_commitment_by_version(
            &block.body.transactions,
            &block.body.transaction_hashes,
            &block_hash_version,
        )
        .expect("Failed to calculate transaction commitment.")
    });

    header.event_commitment.get_or_insert_with(|| {
        let block_hash_version = get_block_hash_version(chain_id, &header.block_number);
        calculate_event_commitment_by_version(
            block.body.transaction_outputs.iter().flat_map(|output| output.events()),
            &block_hash_version,
        )
    });

    header.n_transactions.get_or_insert(block.body.transactions.len());
    header.n_events.get_or_insert_with(|| {
        block.body.transaction_outputs.iter().map(|o| o.events().len()).sum()
    });

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

    if !CentralSourceVerifier::verify_signature(
        &header.block_hash,
        &state_diff_commitment,
        &signature,
        sequencer_pub_key,
    )? {
        panic!("Failed to verify signature for block number {}.", bn);
    }

    Ok(())
}
