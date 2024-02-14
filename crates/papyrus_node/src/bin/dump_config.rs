// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(feature = "rpc")]
#[path = "dump_config/with_rpc.rs"]
mod with_rpc;

#[cfg(feature = "rpc")]
use papyrus_config::dumping::SerializeConfig;
#[cfg(feature = "rpc")]
use papyrus_node::config::{NodeConfig, DEFAULT_CONFIG_PATH};

#[cfg(feature = "rpc")]
use self::with_rpc::CONFIG_POINTERS;

/// Updates the default config file by:
/// cargo run --bin dump_config -q
#[cfg_attr(coverage_nightly, coverage_attribute)]
fn main() {
    #[cfg(feature = "rpc")]
    NodeConfig::default()
        .dump_to_file(&CONFIG_POINTERS, DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
    // TODO(shahak): Try to find a way to remove this binary altogether when the feature rpc is
    // turned off.
    #[cfg(not(feature = "rpc"))]
    panic!("Can't dump config when the rpc feature is deactivated");
}
