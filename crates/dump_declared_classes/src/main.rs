use papyrus_storage::utils::dump_declared_classes_table_to_file;

/// This executable dumps the declared_classes table from the storage to a file.
/// The file path can be passed as an argument, otherwise it will be dumped to
/// "dump_declared_classes.json".
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let default_file_path = "dump_declared_classes.json".to_string();
    let file_path = args.get(1).unwrap_or(&default_file_path);

    match dump_declared_classes_table_to_file(file_path) {
        Ok(_) => println!("Dumped declared_classes table to file: {}", file_path),
        Err(e) => println!("Failed dumping declared_classes table with error: {}", e),
    }
}
