use std::fs::{read_to_string, File};
use std::time::Duration;

use clap::{Arg, Command};
use papyrus_common::storage_query::StorageQuery;
use papyrus_storage::db::DbConfig;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageConfig;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use statistical::median;

// TODO(dvir): consider add logger and use it for the prints.
// TODO(dvir): add this to the readme of the binaries and/or consider reordering the binaries.
// TODO(dvir): consider adding tests.

pub fn main() {
    let cli_params = get_cli_params();

    // Creates List of queries to be executed.
    println!("Creating queries");
    let mut queries: Vec<StorageQuery> = Vec::new();
    for line in read_to_string(cli_params.queries_file_path)
        .expect("Should be able to read the queries file")
        .lines()
    {
        queries
            .push(serde_json::from_str(line).expect("Query should be a valid query json object"));
    }

    // Open storage to execute the queries.
    println!("Opening storage");
    let db_config = DbConfig {
        path_prefix: cli_params.db_path.into(),
        chain_id: ChainId(cli_params.chain_id),
        ..Default::default()
    };
    let config = StorageConfig { db_config, ..Default::default() };

    let (reader, mut _writer) =
        papyrus_storage::open_storage(config).expect("Should be able to open storage");
    let txn = reader.begin_ro_txn().expect("Should be able to begin read only transaction");
    let state_reader = txn.get_state_reader().expect("Should be able to get state reader");

    let mut times = Times::default();

    // Execute the queries and measure the time it takes to execute them.
    println!("Executing queries");
    for q in queries {
        let exec_time;
        match q {
            StorageQuery::GetClassHashAt(state_number, contract_address) => {
                let now = std::time::Instant::now();
                let _class_hash = state_reader.get_class_hash_at(state_number, &contract_address);
                exec_time = now.elapsed();
                times.get_class_hash_at.push(exec_time);
            }
            StorageQuery::GetNonceAt(state_number, contract_address) => {
                let now = std::time::Instant::now();
                let _nonce = state_reader.get_nonce_at(state_number, &contract_address);
                exec_time = now.elapsed();
                times.get_nonce_at.push(exec_time);
            }
            StorageQuery::GetStorageAt(state_number, contract_address, storage_key) => {
                let now = std::time::Instant::now();
                let _storage =
                    state_reader.get_storage_at(state_number, &contract_address, &storage_key);
                exec_time = now.elapsed();
                times.get_storage_at.push(exec_time);
            }
        }
        println!("{}", serde_json::to_string(&q).expect("Should be able to serialize the query"));
        println!("time in microseconds: {}", exec_time.as_micros());
    }

    println!("Writing results to file");
    let results_file = File::create(cli_params.output_file_path)
        .expect("Should be able to create the output file");
    let final_results = times.get_final_results();
    serde_json::to_writer(results_file, &final_results)
        .expect("Should be able to write to the output file");
}

// Records the time it takes to execute the queries.
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Times {
    get_class_hash_at: Vec<Duration>,
    get_nonce_at: Vec<Duration>,
    get_storage_at: Vec<Duration>,
}

impl Times {
    // Returns statics about the executing times of the queries in a format that can be use in
    // github action.
    fn get_final_results(&self) -> Vec<Entry> {
        let mut results: Vec<Entry> = vec![];

        let get_class_hash_at_median = if self.get_class_hash_at.is_empty() {
            0
        } else {
            median(&self.get_class_hash_at.iter().map(|x| x.as_micros()).collect::<Vec<u128>>())
        };
        results.push(Entry {
            name: "get_class_hash_at".to_string(),
            unit: "Microseconds".to_string(),
            value: get_class_hash_at_median as usize,
        });

        let get_nonce_at_median = if self.get_nonce_at.is_empty() {
            0
        } else {
            median(&self.get_nonce_at.iter().map(|x| x.as_micros()).collect::<Vec<u128>>())
        };
        results.push(Entry {
            name: "get_nonce_at".to_string(),
            unit: "Microseconds".to_string(),
            value: get_nonce_at_median as usize,
        });

        let get_storage_at_median = if self.get_storage_at.is_empty() {
            0
        } else {
            median(&self.get_storage_at.iter().map(|x| x.as_micros()).collect::<Vec<u128>>())
        };

        results.push(Entry {
            name: "get_storage_at".to_string(),
            unit: "Microseconds".to_string(),
            value: get_storage_at_median as usize,
        });

        results
    }

    #[allow(dead_code)]
    fn print_times(&self) {
        let get_class_hash_at_time_sum = self.get_class_hash_at.iter().sum::<Duration>();
        let get_nonce_at_time_sum = self.get_nonce_at.iter().sum::<Duration>();
        let get_storage_at_time_sum = self.get_storage_at.iter().sum::<Duration>();

        println!("Times:");
        println!(" - GetClassHashAt: {:?}", get_class_hash_at_time_sum.as_nanos());
        println!(" - GetNonceAt: {:?}", get_nonce_at_time_sum.as_nanos());
        println!(" - GetStorageAt: {:?}", get_storage_at_time_sum.as_nanos());
        println!(
            " - total time: {:?}",
            (get_class_hash_at_time_sum + get_nonce_at_time_sum + get_storage_at_time_sum)
                .as_nanos()
        );
    }
}

// Represents a single entry in the results file.
#[derive(Debug, Clone, Default, Serialize)]
struct Entry {
    name: String,
    unit: String,
    value: usize,
}

struct CliParams {
    queries_file_path: String,
    db_path: String,
    output_file_path: String,
    chain_id: String,
}

fn get_cli_params() -> CliParams {
    let matches = Command::new("Storage benchmark")
        .arg(
            Arg::new("queries_file_path")
                .short('q')
                .long("queries_file_path")
                .required(true)
                .help("The path to a file with the queries"),
        )
        .arg(
            Arg::new("db_path")
                .short('d')
                .long("db_path")
                .required(true)
                .help("The path to the database"),
        )
        .arg(
            Arg::new("output_file_path")
                .short('o')
                .long("output_file_path")
                .required(true)
                .help("The path to the output file"),
        )
        .arg(
            Arg::new("chain_id")
                .short('c')
                .long("chain_id")
                .required(true)
                .help("The chain id SN_MAIN/SN_SEPOLIA for example"),
        )
        .get_matches();

    let queries_file_path = matches
        .get_one::<String>("queries_file_path")
        .expect("Missing queries_file_path")
        .to_string();
    let db_path = matches.get_one::<String>("db_path").expect("Missing db_path").to_string();
    let output_file_path = matches
        .get_one::<String>("output_file_path")
        .expect("Missing output_file_path")
        .to_string();
    let chain_id =
        matches.get_one::<String>("chain_id").expect("Missing parse chain_id").to_string();

    CliParams { queries_file_path, db_path, output_file_path, chain_id }
}
