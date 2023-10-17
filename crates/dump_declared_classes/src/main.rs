use clap::{Arg, Command};
use papyrus_storage::utils::{
    dump_declared_classes_table_by_block_range,
    dump_declared_classes_table_to_file,
};
use papyrus_storage::StorageResult;

/// This executable dumps the declared_classes table from the storage to a file.
/// The file path can be passed as an argument, otherwise it will be dumped to
/// "dump_declared_classes.json".
/// A starting block and an ending block can also be passed as optional arguments, otherwise the
/// entire table will be dumped.
fn main() {
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
                .help("The block number to start dumping from."),
        )
        .arg(
            Arg::new("end_block")
                .short('e')
                .long("end_block")
                .help("The block number to end dumping at."),
        )
        .get_matches();

    let file_path = matches.get_one::<String>("file_path").unwrap().as_str();
    let res: StorageResult<()> =
        if matches.contains_id("start_block") && matches.contains_id("end_block") {
            let start_block = matches
                .get_one::<String>("start_block")
                .unwrap()
                .parse::<u64>()
                .expect("Failed parsing start_block");
            let end_block = matches
                .get_one::<String>("end_block")
                .unwrap()
                .parse::<u64>()
                .expect("Failed parsing end_block");
            dump_declared_classes_table_by_block_range(start_block, end_block, file_path)
        } else {
            dump_declared_classes_table_to_file(file_path)
        };
    match res {
        Ok(_) => println!("Dumped declared_classes table to file: {}", file_path),
        Err(e) => println!("Failed dumping declared_classes table with error: {}", e),
    }
}
