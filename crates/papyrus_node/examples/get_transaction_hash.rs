use std::cmp::min;
use std::collections::{BTreeMap, HashSet};

use clap::{Arg, Command};
use papyrus_common::transaction_hash::{
    get_transaction_hash,
    MAINNET_TRANSACTION_HASH_WITH_VERSION,
};
use papyrus_common::TransactionOptions;
use reqwest::Client;
use serde_json::{json, to_writer_pretty, Map, Value};
use starknet_api::core::ChainId;
use starknet_api::transaction::{self, Transaction};
use starknet_client::reader::objects::transaction::TransactionType;
use strum::IntoEnumIterator;

const DEFAULT_TRANSACTION_HASH_PATH: &str =
    "crates/papyrus_common/resources/transaction_hash_new.json";
struct CliParams {
    node_url: String,
    iteration_increments: u64,
    file_path: String,
    deprecated: bool,
}

/// The start_block and end_block arguments are mandatory and define the block range to dump,
/// start_block is inclusive and end_block is exclusive. The file_path is an optional parameter,
/// otherwise the data will be dumped to "dump_declared_classes.json".
fn get_cli_params() -> CliParams {
    let matches = Command::new("Get transaction hash")
        .arg(
            Arg::new("file_path")
                .short('f')
                .long("file_path")
                .default_value(DEFAULT_TRANSACTION_HASH_PATH)
                .help("The file path to dump the transactions."),
        )
        .arg(
            Arg::new("node_url")
                .short('n')
                .long("node_url")
                .required(true)
                .help("The node url to query."),
        )
        .arg(
            Arg::new("iteration_increments")
                .short('i')
                .long("iteration_increments")
                .default_value("1")
                .help("The iteration increments used to query the node."),
        )
        .arg(
            Arg::new("deprecated")
                .short('d')
                .long("deprecated")
                .default_value("false")
                .help("Create a dump of deprecated transactions."),
        )
        .get_matches();

    let file_path =
        matches.get_one::<String>("file_path").expect("Failed parsing file_path").to_string();
    let node_url =
        matches.get_one::<String>("node_url").expect("Failed parsing node_url").to_string();
    let iteration_increments = matches
        .get_one::<String>("iteration_increments")
        .expect("Failed parsing iteration_increments")
        .parse::<u64>()
        .expect("Failed parsing iteration_increments");
    let deprecated = matches
        .get_one::<String>("deprecated")
        .expect("Failed parsing deprecated")
        .parse::<bool>()
        .expect("Failed parsing deprecated");
    CliParams { node_url, iteration_increments, file_path, deprecated }
}

// Define a tuple struct to hold transaction type and version
#[derive(Eq, PartialEq, Hash, Debug)]
struct TransactionInfo {
    pub transaction_type: TransactionType,
    pub transaction_version: String,
}

fn get_all_transaction_types() -> HashSet<TransactionInfo> {
    let mut enum_values = HashSet::new();
    let versions = ["0x0", "0x1", "0x2", "0x3"];
    for transaction_type in TransactionType::iter() {
        for version in versions.iter() {
            enum_values.insert(TransactionInfo {
                transaction_type,
                transaction_version: version.to_string(),
            });
        }
    }
    enum_values
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Starknet transaction hash dump.");

    let CliParams { node_url, iteration_increments, file_path, deprecated } = get_cli_params();
    let file = std::fs::File::create(file_path)?;
    let mut writer = std::io::BufWriter::new(&file);

    let mut transaction_types = get_all_transaction_types();
    let mut acumulated_transactions = vec![];

    let client = reqwest::Client::new();
    let mut block_number: u64 = if deprecated {
        MAINNET_TRANSACTION_HASH_WITH_VERSION.0
    } else {
        get_current_block_number_via_rpc(&client, node_url.clone()).await?
    };

    while block_number > 0 && !transaction_types.is_empty() {
        println!("Processing block number: {}", block_number);
        let block_transactions =
            get_block_transactions_via_rpc(&client, node_url.clone(), block_number).await?;

        // For each transaction in the block, check if it's a unique transaction type and version
        // and add it to the acumulated_transactions
        for transaction in block_transactions.iter().cloned() {
            let transaction_info = parse_transaction_info_from_value(&transaction);
            if transaction_types.remove(&transaction_info) {
                let unique_transaction = construct_transaction_from_value(
                    transaction.clone(),
                    &transaction_info.transaction_type,
                    &transaction_info.transaction_version,
                )?;
                let transaction_hash = transaction["transaction_hash"]
                    .as_str()
                    .expect("Couldn't parse 'transaction_hash' from json transaction")
                    .to_string();

                let transaction_map = create_map_of_transaction(
                    &unique_transaction,
                    block_number,
                    transaction_hash,
                    deprecated,
                );
                acumulated_transactions.push(transaction_map);
            }
        }

        // Decrement the block number by the iteration_increments
        block_number -= min(iteration_increments, block_number);
    }
    to_writer_pretty(&mut writer, &acumulated_transactions)?;
    println!("Transaction hash dump completed.");
    Ok(())
}

fn create_map_of_transaction(
    transaction: &Transaction,
    block_number: u64,
    transaction_hash: String,
    deprecated: bool,
) -> BTreeMap<String, Value> {
    let chain_id = ChainId("SN_MAIN".to_string());
    let mut transaction_info = BTreeMap::new();
    transaction_info.insert("transaction".to_string(), json!(transaction));
    transaction_info.insert("chain_id".to_string(), json!(chain_id));
    transaction_info.insert("block_number".to_string(), json!(block_number));
    transaction_info.insert("transaction_hash".to_string(), json!(transaction_hash));
    // If the transaction is deprecated, only the transaction hash is needed
    if !deprecated {
        if let Transaction::L1Handler(_) = transaction {
            return transaction_info;
        }
        // Note that we test the only_query_transaction_hash using the same method thats used to
        // insert the only_query_transaction_hash into the json file.
        transaction_info.insert(
            "only_query_transaction_hash".to_string(),
            json!(
                get_transaction_hash(
                    transaction,
                    &chain_id,
                    &TransactionOptions { only_query: true }
                )
                .expect("Couldn't get only query transaction hash")
            ),
        );
    }
    transaction_info
}
fn construct_transaction_from_value(
    mut transaction: Value,
    transaction_type: &TransactionType,
    transaction_version: &str,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    println!(
        "Constructing transaction from type: {} {}",
        serde_json::to_string(transaction_type).expect("Couldn't parse transaction type"),
        transaction_version
    );
    let transaction_map =
        transaction.as_object_mut().expect("Couldn't parse json transaction into object");
    if transaction_map.contains_key("resource_bounds") {
        if let Some(resource_bounds) = transaction_map.remove("resource_bounds") {
            let mut updated_resource_bounds = Map::new();
            for (key, value) in resource_bounds
                .as_object()
                .expect("Couldn't parse json value `resource_bounds` into object")
            {
                updated_resource_bounds.insert(key.clone().to_ascii_uppercase(), value.clone());
            }
            transaction_map.insert("resource_bounds".to_string(), json!(updated_resource_bounds));
        }
    }
    match transaction_type {
        TransactionType::Declare => match transaction_version {
            "0x0" => Ok(Transaction::Declare(transaction::DeclareTransaction::V0(
                serde_json::from_value(transaction)?,
            ))),
            "0x1" => Ok(Transaction::Declare(transaction::DeclareTransaction::V1(
                serde_json::from_value(transaction)?,
            ))),
            "0x2" => Ok(Transaction::Declare(transaction::DeclareTransaction::V2(
                serde_json::from_value(transaction)?,
            ))),
            "0x3" => Ok(Transaction::Declare(transaction::DeclareTransaction::V3(
                serde_json::from_value(transaction)?,
            ))),
            _ => Err("Invalid transaction version".into()),
        },
        TransactionType::InvokeFunction => match transaction_version {
            "0x0" => Ok(Transaction::Invoke(transaction::InvokeTransaction::V0(
                serde_json::from_value(transaction)?,
            ))),
            "0x1" => Ok(Transaction::Invoke(transaction::InvokeTransaction::V1(
                serde_json::from_value(transaction)?,
            ))),
            "0x3" => Ok(Transaction::Invoke(transaction::InvokeTransaction::V3(
                serde_json::from_value(transaction)?,
            ))),
            _ => Err("Invalid transaction version".into()),
        },
        TransactionType::DeployAccount => match transaction_version {
            "0x1" => Ok(Transaction::DeployAccount(transaction::DeployAccountTransaction::V1(
                serde_json::from_value(transaction)?,
            ))),
            "0x3" => Ok(Transaction::DeployAccount(transaction::DeployAccountTransaction::V3(
                serde_json::from_value(transaction)?,
            ))),
            _ => Err("Invalid transaction version".into()),
        },
        TransactionType::Deploy => Ok(Transaction::Deploy(serde_json::from_value(transaction)?)),
        TransactionType::L1Handler => {
            Ok(Transaction::L1Handler(serde_json::from_value(transaction)?))
        }
    }
}

// JSON RPC for block transactions
async fn get_block_transactions_via_rpc(
    client: &Client,
    node_url: String,
    block_number: u64,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "starknet_getBlockWithTxs",
        "params": {
            "block_id": {"block_number": block_number}
        }
    });
    let res: reqwest::Response = client
        .post(node_url)
        .header("Content-Type", "application/json")
        .body(request_body.to_string())
        .send()
        .await?;

    // Check if the request was successful
    if !res.status().is_success() {
        // Handle the error if the request was not successful
        return Err(format!("Request failed with status code: {}", res.status()).into());
    }
    let value = res.json::<Value>().await?;
    let block_transactions = value["result"]["transactions"]
        .as_array()
        .expect("Couldn't parse json result into array.")
        .clone();
    Ok(block_transactions)
}

// JSON RPC for current block number
async fn get_current_block_number_via_rpc(
    client: &Client,
    node_url: String,
) -> Result<u64, Box<dyn std::error::Error>> {
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "starknet_blockNumber",
        "params": {
        }
    });

    // Send the HTTP POST request
    let res: reqwest::Response = client
        .post(node_url)
        .header("Content-Type", "application/json")
        .body(request_body.to_string())
        .send()
        .await?;

    // Check if the request was successful
    if !res.status().is_success() {
        // Handle the error if the request was not successful
        return Err(format!("Request failed with status code: {}", res.status()).into());
    }
    let res = res.json::<Value>().await?;
    res["result"].as_u64().ok_or_else(|| ("Couldn't parse json response.").into())
}

fn parse_transaction_info_from_value(transaction: &Value) -> TransactionInfo {
    let transaction_type = transaction["type"].clone();
    if transaction_type.as_str().expect("Couldn't parse 'type' from transaction") == "INVOKE" {
        return TransactionInfo {
            transaction_type: TransactionType::InvokeFunction,
            transaction_version: transaction["version"]
                .as_str()
                .expect("Couldn't parse 'version' from json transaction")
                .to_string(),
        };
    }
    TransactionInfo {
        transaction_type: serde_json::from_value(transaction_type)
            .expect("Couldn't parse 'type' from json transaction"),
        transaction_version: transaction["version"]
            .as_str()
            .expect("Couldn't parse 'version' from json transaction")
            .to_string(),
    }
}
