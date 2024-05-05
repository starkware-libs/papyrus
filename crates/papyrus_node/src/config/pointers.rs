use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::discriminant;
use std::ops::IndexMut;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use clap::{arg, value_parser, Arg, ArgMatches, Command};
use itertools::{chain, Itertools};
use lazy_static::lazy_static;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
#[cfg(not(feature = "rpc"))]
use papyrus_config::dumping::ser_param;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_sub_config,
    ser_pointer_target_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
#[cfg(not(feature = "rpc"))]
use papyrus_config::ParamPrivacyInput;
use papyrus_config::{ConfigError, ParamPath, SerializedParam};
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_network::NetworkConfig;
use papyrus_p2p_sync::{P2PSync, P2PSyncConfig};
#[cfg(feature = "rpc")]
use papyrus_rpc::RpcConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::sources::central::CentralSourceConfig;
use papyrus_sync::SyncConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;
use validator::Validate;

use crate::version::VERSION_FULL;

lazy_static! {
    /// Returns vector of (pointer target name, pointer target serialized param, vec<pointer param path>)
    /// to be applied on the dumped node config.
    /// The config updates will be performed on the shared pointer targets, and finally, the values
    /// will be propagated to the pointer params.
    pub static ref CONFIG_POINTERS: Vec<((ParamPath, SerializedParam), Vec<ParamPath>)> = vec![(
        ser_pointer_target_param(
            "chain_id",
            &ChainId("SN_MAIN".to_string()),
            "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
        ),
        vec!["storage.db_config.chain_id".to_owned(), "rpc.chain_id".to_owned(), "sync.chain_id".to_owned()],
    ),
    (
        ser_pointer_target_param(
            "starknet_url",
            &"https://alpha-mainnet.starknet.io/".to_string(),
            "The URL of a centralized Starknet gateway.",
        ),
        vec!["rpc.starknet_url".to_owned(), "central.url".to_owned(), "monitoring_gateway.starknet_url".to_owned()],
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
