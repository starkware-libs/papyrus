use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Map, Value};
pub type ParamPath = String;
pub type SerializedValue = String;
pub type Description = String;

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";

pub trait SubConfig: Serialize {
    fn config_name() -> String;

    /// A mapping between a param name to its description.
    fn fields_description() -> HashMap<String, Description>;

    fn param_path(param_name: &String) -> ParamPath {
        let config_name = Self::config_name();
        format!("{config_name}.{param_name}")
    }

    /// Serializes the sub-config into JSON.
    fn dumps(&self) -> Map<String, Value> {
        let descriptions = Self::fields_description();
        let json_map: Map<String, Value> = serde_json::to_value(self)
            .expect("Unable to serialize sub-config")
            .as_object()
            .expect("Unable to convert sub-config to map")
            .clone();

        let mut described_json_map = Map::<String, Value>::new();
        for (key, value) in json_map {
            let described_value = json!({
                "description": descriptions
                    .get(&key)
                    .expect("Missing key from sub-config descriptions")
                    .to_owned(),
                "value": value
            });
            described_json_map.insert(Self::param_path(&key), described_value);
        }
        described_json_map
    }
}
