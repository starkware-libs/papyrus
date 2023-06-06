use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::IndexMut;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type ParamPath = String;
pub type Description = String;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    pub description: String,
    pub value: Value,
}

#[derive(thiserror::Error, Debug)]
pub enum SubConfigError {
    #[error(transparent)]
    MissingParam(#[from] serde_json::Error),
}
/// Serialization and deserialization for configs.
/// For an explanation of `for<'a> Deserialize<'a>` see https://doc.rust-lang.org/nomicon/hrtb.html.
pub trait SerdeConfig: for<'a> Deserialize<'a> + Serialize + Sized {
    fn config_name() -> String;

    /// Serializes the config into flatten JSON.
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam>;

    /// Serializes the config into flatten JSON, where the path of each param is rooted at this
    /// config name.
    /// Used by `dump` when a field itself implements `SerdeConfig`.
    fn dump_sub_config(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let descriptions = self.dump();
        let mut named_map = BTreeMap::new();

        for (field_name, val) in descriptions {
            named_map.insert(param_path::<Self>(&field_name), val);
        }
        named_map
    }

    /// Deserializes the sub-config from flatten JSON.
    fn load(config_dump: &BTreeMap<ParamPath, SerializedParam>) -> Result<Self, SubConfigError> {
        let mut nested_map = json!({});
        for (param_path, serialized_param) in config_dump {
            let mut entry = &mut nested_map;
            for config_name in param_path.split(".") {
                entry = entry.index_mut(config_name);
            }
            *entry = serialized_param.value.clone();
        }
        Ok(serde_json::from_value(nested_map)?)
    }

    /// Deserializes the sub-config from flatten JSON. Takes the params that are rooted at this
    /// config name.
    fn load_sub_config(
        config_dump: &BTreeMap<ParamPath, SerializedParam>,
    ) -> Result<Self, SubConfigError> {
        let prefix = format!("{}.", Self::config_name());

        let mut filtered_map = BTreeMap::<ParamPath, SerializedParam>::new();
        for (key, value) in config_dump {
            if let Some(param_name) = key.strip_prefix(&prefix) {
                filtered_map.insert(param_name.to_owned(), value.to_owned());
            }
        }
        Self::load(&filtered_map)
    }
}

fn param_path<T: SerdeConfig>(param_name: &str) -> ParamPath {
    let config_name = T::config_name();
    format!("{config_name}.{param_name}")
}

pub fn dump_sub_config_to_file<T: SerdeConfig>(config: &T, file_path: &str) {
    let dumped = config.dump_sub_config();
    let file = File::create(file_path).expect("creating failed");
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &dumped).expect("writing failed");
    writer.flush().expect("flushing failed");
}
