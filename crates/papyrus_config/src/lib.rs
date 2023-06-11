use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;
pub type ParamPath = String;
pub type SerializedValue = Value;
pub type Description = String;

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    pub description: String,
    pub value: Value,
}

/// Serialization and deserialization for configs.
pub trait SerdeConfig: Serialize + Sized {
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
