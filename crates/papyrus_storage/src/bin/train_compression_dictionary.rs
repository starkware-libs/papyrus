use std::fs::File;
use std::io::Write;

use clap::{Arg, Command};
use papyrus_storage::db::serialization::StorageSerde;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{open_storage, StorageReader};
use rand::Rng;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::state::ThinStateDiff;

// TODO(dvir): add readme for this binary.
// TODO(dvir): consider adding an option to train a dictionary by getting data from the RPC.
// TODO(dvir): consider adding tests.
// TODO(dvir): consider add logger instead of printing.

// In zstd the training set size is limited to 2GB.
const TRAINING_SET_SIZE_LIMIT: usize = 1 << 31; // 2GB

fn main() {
    let cli_params = get_cli_params();
    let storage_reader = get_reader(&cli_params.db_path, &cli_params.chain_id);

    // A function that returns bytes of serialized objects in the given block range.
    let objects_generator: fn(&StorageReader, &BlockNumber, &BlockNumber) -> Vec<u8> =
        match cli_params.object_type {
            ObjectType::ThinStateDiff => ThinStateDiff::get_random_object_bytes,
        };

    let start_block = BlockNumber(cli_params.start_block);
    let end_block = BlockNumber(cli_params.end_block);

    let mut sample_data = Vec::new();
    let mut sample_sizes = Vec::new();

    while sample_data.len() < TRAINING_SET_SIZE_LIMIT {
        let data = objects_generator(&storage_reader, &start_block, &end_block);
        sample_sizes.push(data.len());
        sample_data.extend(data);
    }

    println!("data samples number: {}", sample_sizes.len());
    println!("total data bytes: {}", sample_sizes.iter().sum::<usize>());

    let dictionary =
        zstd::dict::from_continuous(&sample_data, &sample_sizes, cli_params.max_dict_size)
            .expect("Should be able to train the dictionary.");
    println!("dictionary size: {}", dictionary.len());

    let mut target_file =
        File::create(cli_params.file_path).expect("Should be able to create the target file.");
    target_file
        .write_all(&dictionary)
        .expect("Should be able to write the dictionary to the target file.");
}

// A trait that defines a function to get a random object bytes in the given block range.
trait GetRandomObject {
    fn get_random_object_bytes(
        storage_reader: &StorageReader,
        start_block: &BlockNumber,
        end_block: &BlockNumber,
    ) -> Vec<u8>;
}

impl GetRandomObject for ThinStateDiff {
    fn get_random_object_bytes(
        storage_reader: &StorageReader,
        start_block: &BlockNumber,
        end_block: &BlockNumber,
    ) -> Vec<u8> {
        let random_block_number =
            BlockNumber(rand::thread_rng().gen_range(start_block.0..end_block.0));
        let txn =
            storage_reader.begin_ro_txn().expect("Should be able to begin read only transaction");
        let state_diff = txn
            .get_state_diff(random_block_number)
            .unwrap_or_else(|_| {
                panic!("Should be able to get state diff at block {random_block_number}")
            })
            .unwrap_or_else(|| {
                panic!("Should be able to get state diff at block {random_block_number}")
            });

        println!("Gets state diff of block {}", random_block_number.0);

        let mut bytes = Vec::new();
        state_diff.serialize_into(&mut bytes).expect("Should be able to serialize state diff.");
        bytes
    }
}

fn get_reader(db_path: &str, chain_id: &str) -> StorageReader {
    let mut config = papyrus_storage::StorageConfig::default();
    config.db_config.path_prefix = db_path.into();
    config.db_config.chain_id = ChainId(chain_id.into());
    open_storage(config).expect("Should be able to open storage").0
}

struct CliParams {
    start_block: u64,
    end_block: u64,
    file_path: String,
    chain_id: String,
    max_dict_size: usize,
    object_type: ObjectType,
    db_path: String,
}

enum ObjectType {
    ThinStateDiff,
}

impl From<&str> for ObjectType {
    fn from(s: &str) -> Self {
        match s {
            "thin_state_diff" => ObjectType::ThinStateDiff,
            _ => panic!("Invalid object type: {}", s),
        }
    }
}

fn get_cli_params() -> CliParams {
    let matches = Command::new("Train compression dictionary")
        .arg(
            Arg::new("db_path")
                .short('d')
                .long("db_path")
                .help("The path to the database with the data."),
        )
        .arg(
            Arg::new("file_path")
                .short('f')
                .long("file_path")
                .help("The file path to write the dictionary to."),
        )
        .arg(
            Arg::new("chain_id")
                .short('c')
                .long("chain_id")
                .required(true)
                .help("The chain id SN_MAIN/SN_GOERLI."),
        )
        .arg(
            Arg::new("start_block")
                .short('s')
                .long("start_block")
                .default_value("0")
                .help("The block number to start training from."),
        )
        .arg(
            Arg::new("end_block")
                .short('e')
                .long("end_block")
                .default_value(u64::MAX.to_string())
                .help("The block number to end training at."),
        )
        .arg(
            Arg::new("max_dictionary_size")
                .short('m')
                .long("max_dict_size")
                .required(true)
                .help("The max dictionary size in bytes."),
        )
        .arg(
            Arg::new("object_type")
                .short('t')
                .long("object_type")
                .required(true)
                .help("The object type to train the dictionary for.\nOne of: thin_state_diff."),
        )
        .get_matches();

    let db_path = matches.get_one::<String>("db_path").expect("Missing db_path").to_string();
    let chain_id = matches.get_one::<String>("chain_id").expect("Missing chain_id").to_string();
    let file_path = matches.get_one::<String>("file_path").expect("Missing file_path").to_string();
    let start_block = matches
        .get_one::<String>("start_block")
        .expect("Missing start_block")
        .parse::<u64>()
        .expect("Failed parsing start_block");
    let end_block = matches
        .get_one::<String>("end_block")
        .expect("Missing end_block")
        .parse::<u64>()
        .expect("Failed parsing end_block");
    if start_block >= end_block {
        panic!("start_block must be smaller than end_block");
    }
    let max_dict_size = matches
        .get_one::<String>("max_dictionary_size")
        .expect("Missing max_dict_size")
        .parse::<usize>()
        .expect("Failed parsing max_dict_size");
    let object_type =
        matches.get_one::<String>("object_type").expect("Missing object_type").as_str().into();
    CliParams { start_block, end_block, file_path, chain_id, max_dict_size, object_type, db_path }
}
