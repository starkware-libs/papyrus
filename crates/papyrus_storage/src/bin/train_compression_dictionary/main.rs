#[cfg(test)]
#[path = "train_compression_dictionary_test.rs"]
mod train_compression_dictionary_test;

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use clap::{Arg, Command};
use papyrus_storage::db::serialization::StorageSerde;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{open_storage, StorageReader};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use tempfile::TempDir;

// TODO(dvir): add readme for this binary.

const DATA_TEMP_FILE_NAME_PREFIX: &str = "temp_data_";
const DATA_TEMP_FILE_NAME_SUFFIX: &str = ".dat";
// TODO(dvir): fine tune this value.
// If a file is bigger than this size, a new file will be started.
const DATA_FILE_SIZE_THRESHOLD: usize = 1 << 20; // 1MB

fn main() {
    let cli_params = get_cli_params();
    let storage_reader = get_reader(&cli_params.db_path, &cli_params.chain_id);
    // All the temporary files with the serialized data will be placed here and will be deleted
    // automatically when tempdir will be dropped.
    let temp_dir = TempDir::new().expect("Failed creating temp dir");
    let data_files = match cli_params.object_type {
        ObjectType::ThinStateDiffType => {
            let iter = ThinStateDiffIterator::new(
                BlockNumber(cli_params.start_block),
                BlockNumber(cli_params.end_block),
                &storage_reader,
            );
            create_data_files(&temp_dir, iter, DATA_FILE_SIZE_THRESHOLD)
        }
    };

    // For some reason the training fails if the number of files is too small.
    let dict = zstd::dict::from_files(&data_files, cli_params.max_dict_size).expect(
        "
    There are not enough data files to train a dictionary. Train on more data or increase the \
         value of DATA_FILE_SIZE_THRESHOLD constant in the code.\nOriginal error message",
    );
    let mut target_file = File::create(cli_params.file_path).expect("Failed creating file");
    target_file.write_all(&dict).expect("Failed writing to file");

    let mut total_data_bytes = 0;
    for file in data_files.iter() {
        total_data_bytes += std::fs::metadata(file).expect("Failed getting metadata").len();
    }
    println!("total data bytes: {total_data_bytes}");
    println!("total data files: {}", data_files.len());
    println!("dictionary size: {}", dict.len());
}

// Creates the data files and returns their paths.
fn create_data_files<S: StorageSerde>(
    dir: &TempDir,
    iter: impl Iterator<Item = S>,
    file_size_limit: usize,
) -> Vec<PathBuf> {
    let mut file_paths = Vec::new();
    let mut file_idx = 0;
    let mut path = get_data_file_path(dir, file_idx);
    let mut file = File::create(&path).expect("Failed creating file");
    file_paths.push(path);
    for item in iter {
        if file.metadata().expect("Failed getting metadata").len() >= file_size_limit as u64 {
            file_idx += 1;
            path = get_data_file_path(dir, file_idx);
            file = File::create(&path).expect("Failed creating file");
            file_paths.push(path);
        }
        // NOTICE: serialize_into is used here so if this function includes also compression
        // there is need to decompress the data before training.
        item.serialize_into(&mut file).expect("Failed serialization to file");
    }
    file_paths
}

fn get_data_file_path(dir: &TempDir, idx: usize) -> PathBuf {
    dir.path().join(
        DATA_TEMP_FILE_NAME_PREFIX.to_string() + &idx.to_string() + DATA_TEMP_FILE_NAME_SUFFIX,
    )
}

fn get_reader(db_path: &str, chain_id: &str) -> StorageReader {
    let mut config = papyrus_storage::StorageConfig::default();
    config.db_config.path_prefix = db_path.into();
    config.db_config.chain_id = ChainId(chain_id.into());
    open_storage(config).expect("Failed opening storage").0
}

// TODO(dvir): consider make this more efficient byusing cursor.
struct ThinStateDiffIterator<'reader> {
    end_block: BlockNumber,
    current_block: BlockNumber,
    reader: &'reader StorageReader,
}

impl<'reader> ThinStateDiffIterator<'reader> {
    fn new(
        start_block: BlockNumber,
        end_block: BlockNumber,
        reader: &'reader StorageReader,
    ) -> Self {
        Self { end_block, current_block: start_block, reader }
    }
}

impl<'reader> Iterator for ThinStateDiffIterator<'reader> {
    type Item = starknet_api::state::ThinStateDiff;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_block >= self.end_block {
            return None;
        }
        let state_diff = self
            .reader
            .begin_ro_txn()
            .expect("Failed beginning read transaction.")
            .get_state_diff(self.current_block)
            .expect("Failed getting state diff");
        self.current_block = self.current_block.next();
        state_diff
    }
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
    ThinStateDiffType,
}

impl From<&str> for ObjectType {
    fn from(s: &str) -> Self {
        match s {
            "thin_state_diff" => ObjectType::ThinStateDiffType,
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

    let db_path = matches.get_one::<String>("db_path").expect("Failed parsing db_path").to_string();
    let chain_id =
        matches.get_one::<String>("chain_id").expect("Failed parsing chain_id").to_string();
    let file_path =
        matches.get_one::<String>("file_path").expect("Failed parsing file_path").to_string();
    let start_block = matches
        .get_one::<String>("start_block")
        .expect("Failed parsing start_block")
        .parse::<u64>()
        .expect("Failed parsing start_block");
    let end_block = matches
        .get_one::<String>("end_block")
        .expect("Failed parsing end_block")
        .parse::<u64>()
        .expect("Failed parsing end_block");
    if start_block >= end_block {
        panic!("start_block must be smaller than end_block");
    }
    let max_dict_size = matches
        .get_one::<String>("max_dictionary_size")
        .expect("Failed parsing max_dict_size")
        .parse::<usize>()
        .expect("Failed parsing max_dict_size");
    let object_type = matches
        .get_one::<String>("object_type")
        .expect("Failed parsing object_type")
        .as_str()
        .into();
    CliParams { start_block, end_block, file_path, chain_id, max_dict_size, object_type, db_path }
}
