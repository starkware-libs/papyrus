use std::fs::File;
use std::io::{BufWriter, Write};

use papyrus_config::SerializeConfig;
use papyrus_node::config::{Config, DEFAULT_CONFIG_FILE};

/// Updates the default config file by:
/// cargo run --bin dump_config -q
fn main() {
    let dumped = Config::default().dump();
    let file = File::create(DEFAULT_CONFIG_FILE).expect("creating failed");
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &dumped).expect("writing failed");
    writer.flush().expect("flushing failed");
}
