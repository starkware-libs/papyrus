use clap::{Arg, Command};
use papyrus_storage::utils::dump_declared_classes_table_by_block_range;
use starknet_api::core::ChainId;

/// This executable dumps the declared_classes table from the storage to a file.
fn main() {
    let cli_params = get_cli_params();
    match dump_declared_classes_table_by_block_range(
        cli_params.start_block,
        cli_params.end_block,
        &cli_params.file_path,
        &cli_params.chain_id,
    ) {
        Ok(_) => println!("Dumped declared_classes table to file: {} .", cli_params.file_path),
        Err(e) => println!("Failed dumping declared_classes table with error: {}", e),
    }
}

struct CliParams {
    start_block: u64,
    end_block: u64,
    file_path: String,
    chain_id: ChainId,
}

/// The start_block and end_block arguments are mandatory and define the block range to dump,
/// start_block is inclusive and end_block is exclusive. The file_path is an optional parameter,
/// otherwise the data will be dumped to "dump_declared_classes.json".
fn get_cli_params() -> CliParams {
    let matches = Command::new("Dump declared classes")
        .arg(
            Arg::new("file_path")
                .short('f')
                .long("file_path")
                .default_value("dump_declared_classes.json")
                .help("The file path to dump the declared classes table to."),
        )
        .arg(
            Arg::new("start_block")
                .short('s')
                .long("start_block")
                .required(true)
                .help("The block number to start dumping from."),
        )
        .arg(
            Arg::new("end_block")
                .short('e')
                .long("end_block")
                .required(true)
                .help("The block number to end dumping at."),
        )
        .arg(
            Arg::new("chain_id")
                .short('c')
                .long("chain_id")
                .required(true)
                .help("The chain id SN_MAIN/SN_SEPOLIA, default value is SN_MAIN."),
        )
        .get_matches();

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
    let chain_id =
        matches.get_one::<String>("chain_id").expect("Failed parsing chain_id").to_string();
    CliParams { start_block, end_block, file_path, chain_id: ChainId::Other(chain_id) }
}
