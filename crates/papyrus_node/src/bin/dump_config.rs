use lazy_static::lazy_static;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_node::config::{NodeConfig, DEFAULT_CONFIG_PATH};
use starknet_api::core::ChainId;

lazy_static! {
    /// Returns vector of (pointer target name, pointer target serialized param, vec<pointer param path>)
    /// to be applied on the dumped node config.
    /// The config updates will be performed on the shared pointer targets, and finally, the values
    /// will be propagated to the pointer params.
    static ref CONFIG_POINTERS: Vec<((ParamPath, SerializedParam), Vec<ParamPath>)> = vec![(
        ser_param(
            "chain_id",
            &ChainId("SN_MAIN".to_string()),
            "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
        ),
        vec!["storage.db_config.chain_id".to_owned(), "gateway.chain_id".to_owned()],
    )];
}

/// Updates the default config file by:
/// cargo run --bin dump_config -q
fn main() {
    NodeConfig::default()
        .dump_to_file(&CONFIG_POINTERS, DEFAULT_CONFIG_PATH)
        .expect("dump to file error");
}
