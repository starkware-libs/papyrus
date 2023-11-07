// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use lazy_static::lazy_static;
use papyrus_config::dumping::{ser_pointer_target_param, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_node::config::{NodeConfig, DEFAULT_CONFIG_PATH};
use starknet_api::core::ChainId;

lazy_static! {
    /// Returns vector of (pointer target name, pointer target serialized param, vec<pointer param path>)
    /// to be applied on the dumped node config.
    /// The config updates will be performed on the shared pointer targets, and finally, the values
    /// will be propagated to the pointer params.
    static ref CONFIG_POINTERS: Vec<((ParamPath, SerializedParam), Vec<ParamPath>)> = vec![(
        ser_pointer_target_param(
            "chain_id",
            &ChainId("SN_MAIN".to_string()),
            "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
        ),
        vec!["storage.db_config.chain_id".to_owned(), "rpc.chain_id".to_owned()],
    ),
    (
        ser_pointer_target_param(
            "gateways_url",
            &"https://alpha-mainnet.starknet.io/".to_string(),
            "The url for the gateway and the feeder gateway.",
        ),
        vec!["rpc.starknet_url".to_owned(), "central.url".to_owned()],
    ),
    (
        ser_pointer_target_param(
            "collect_metrics",
            &false,
            "If true, collect metrics for the node.",
        ),
        vec!["rpc.collect_metrics".to_owned(), "monitoring_gateway.collect_metrics".to_owned()],
    )];
}

/// Updates the default config file by:
/// cargo run --bin dump_config -q
#[cfg_attr(coverage_nightly, coverage_attribute)]
fn main() {
    NodeConfig::default()
        .dump_to_file(&CONFIG_POINTERS, DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
}
