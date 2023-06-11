use std::collections::HashMap;

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

pub trait SubConfig: Serialize {
    fn config_name() -> String;

    fn param_path(param_name: &String) -> ParamPath {
        let config_name = Self::config_name();
        format!("{config_name}.{param_name}")
    }

    /// Serializes the config into flatten JSON.
    fn dump(&self) -> HashMap<ParamPath, SerializedParam>;

    /// Serializes the sub-config into flatten JSON.
    /// The path of each param is rooted at this config name.
    /// Used by `dump` when a field itself implements `SubConfig`.
    fn dump_sub_config(&self) -> HashMap<ParamPath, SerializedParam> {
        let descriptions = self.dump();
        let mut named_map = HashMap::new();

        for (field_name, val) in descriptions {
            named_map.insert(Self::param_path(&field_name), val);
        }
        named_map
    }
}
