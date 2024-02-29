// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use papyrus_config::dumping::SerializeConfig;
use papyrus_node::config::{NodeConfig, CONFIG_POINTERS, DEFAULT_CONFIG_PATH};

/// Updates the default config file by:
/// cargo run --bin dump_config -q
#[cfg_attr(coverage_nightly, coverage_attribute)]
fn main() {
    NodeConfig::default()
        .dump_to_file(&CONFIG_POINTERS, DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
}
