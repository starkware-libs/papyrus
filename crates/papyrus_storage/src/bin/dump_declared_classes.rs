use papyrus_storage::utils::dump_declared_classes_table_to_file;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let default_file_path = "dump_declared_classes.json".to_string();
    let file_path = args.get(1).unwrap_or(&default_file_path);

    match dump_declared_classes_table_to_file(file_path) {
        Ok(_) => println!("Dumped declared classes table to file: {}", file_path),
        Err(e) => println!("Failed dumping declared classes with error: {}", e),
    }
}
