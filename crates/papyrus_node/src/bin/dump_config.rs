use std::fs::File;
use std::io::{BufWriter, Write};

use papyrus_config::{combine_config_map_and_pointers, ParamPath, SerializeConfig};
use papyrus_node::config::{Config, DEFAULT_CONFIG_FILE};

/// Updates the default config file by:
/// cargo run --bin dump_config -q
fn main() {
    let dumped = Config::default().dump();
    let combined_map = combine_config_map_and_pointers(dumped, get_pointers()).unwrap();
    let file = File::create(DEFAULT_CONFIG_FILE).expect("creating failed");
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &combined_map).expect("writing failed");
    writer.flush().expect("flushing failed");
}

fn get_pointers() -> Vec<(ParamPath, String, Vec<ParamPath>)> {
    vec![(
        "chain_id".to_owned(),
        "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.".to_owned(),
        vec!["storage.db_config.chain_id".to_owned(), "gateway.chain_id".to_owned()],
    )]
}
