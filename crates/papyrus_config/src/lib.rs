use std::collections::{BTreeMap, HashMap};
use std::mem::discriminant;
use std::ops::IndexMut;
use std::path::PathBuf;
use std::time::Duration;

use clap::parser::MatchesError;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Map, Value};

pub type ParamPath = String;
pub type Description = String;

pub mod command;
#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    pub description: String,
    pub value: Value,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PointerParam {
    pub description: String,
    pub pointer_target: ParamPath,
}

#[derive(thiserror::Error, Debug)]
pub enum SubConfigError {
    #[error(transparent)]
    CommandInput(#[from] clap::error::Error),
    #[error(transparent)]
    MissingParam(#[from] serde_json::Error),
    #[error(transparent)]
    CommandMatches(#[from] MatchesError),
    #[error("Insert a new param is not allowed.")]
    ParamNotFound { param_path: String },
    #[error("{target_param} is not found.")]
    PointerTargetNotFound { target_param: String },
    #[error("{pointing_param} is not found.")]
    PointerSourceNotFound { pointing_param: String },
    #[error("Changing {param_path} type is not allowed.")]
    ChangeParamType { param_path: String },
}
/// Serialization for configs.
pub trait SerializeConfig {
    /// Conversion of a configuration to a mapping of flattened parameters to their descriptions and
    /// values.
    /// Note, in the case of a None sub configs, its elements will not included in the flatten map.
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam>;
}

/// Deserializes config from flatten JSON.
/// For an explanation of `for<'a> Deserialize<'a>` see
/// `<https://doc.rust-lang.org/nomicon/hrtb.html>`.
pub fn load<T: for<'a> Deserialize<'a>>(
    config_dump: &BTreeMap<ParamPath, SerializedParam>,
) -> Result<T, SubConfigError> {
    let mut nested_map = json!({});
    for (param_path, serialized_param) in config_dump {
        let mut entry = &mut nested_map;
        for config_name in param_path.split('.') {
            entry = entry.index_mut(config_name);
        }
        *entry = serialized_param.value.clone();
    }
    Ok(serde_json::from_value(nested_map)?)
}

fn update_config_map(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    param_path: &str,
    new_value: Value,
) -> Result<(), SubConfigError> {
    let Some(serialized_param) = config_map.get_mut(param_path) else {
        return Err(SubConfigError::ParamNotFound{param_path: param_path.to_string()});
    };
    if discriminant(&serialized_param.value) != discriminant(&new_value) {
        return Err(SubConfigError::ChangeParamType { param_path: param_path.to_string() });
    }
    serialized_param.value = new_value;
    Ok(())
}

/// Appends `sub_config_name` to the ParamPath for each entry in `sub_config_dump`.
/// In order to load from a dump properly, `sub_config_name` must match the field's name for the
/// struct this function is called from.
pub fn append_sub_config_name(
    sub_config_dump: BTreeMap<ParamPath, SerializedParam>,
    sub_config_name: &str,
) -> BTreeMap<ParamPath, SerializedParam> {
    BTreeMap::from_iter(
        sub_config_dump
            .into_iter()
            .map(|(field_name, val)| (format!("{}.{}", sub_config_name, field_name), val)),
    )
}

/// Serializes a single param of a config.
/// The returned pair is designed to be an input to a dumped config map.
pub fn ser_param<T: Serialize>(
    name: &str,
    value: &T,
    description: &str,
) -> (String, SerializedParam) {
    (name.to_owned(), SerializedParam { description: description.to_owned(), value: json!(value) })
}

pub fn deserialize_milliseconds_to_duration<'de, D>(de: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let secs: u64 = Deserialize::deserialize(de)?;
    Ok(Duration::from_millis(secs))
}

/// Serializes a map to "k1:v1 k2:v2" string structure.
pub fn serialize_optional_map(optional_map: &Option<HashMap<String, String>>) -> String {
    match optional_map {
        None => "".to_owned(),
        Some(map) => map.iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<String>>().join(" "),
    }
}

/// Deserializes a map from "k1:v1 k2:v2" string structure.
pub fn deserialize_optional_map<'de, D>(de: D) -> Result<Option<HashMap<String, String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_str: String = Deserialize::deserialize(de)?;
    if raw_str.is_empty() {
        return Ok(None);
    }

    let mut map = HashMap::new();
    for raw_pair in raw_str.split(' ') {
        let split: Vec<&str> = raw_pair.split(':').collect();
        if split.len() != 2 {
            return Err(D::Error::custom(format!(
                "pair \"{}\" is not valid. The Expected format is name:value",
                raw_pair
            )));
        }
        map.insert(split[0].to_string(), split[1].to_string());
    }
    Ok(Some(map))
}

fn get_serialized_param(
    param_path: &ParamPath,
    json_map: &Map<String, Value>,
) -> Result<SerializedParam, SubConfigError> {
    if let Some(json_value) = json_map.get(param_path) {
        Ok(serde_json::from_value::<SerializedParam>(json_value.clone())?)
    } else {
        Err(SubConfigError::PointerSourceNotFound { pointing_param: param_path.to_owned() })
    }
}

/// Takes a config map and a vector of {target param, target description, and vector of params that
/// will point to it}.
/// Adds to the map the target params with value of one of the params that points to it.
/// Replaces the value of the pointers to contain only the name of the target they point to.
pub fn combine_config_map_and_pointers(
    config_map: BTreeMap<ParamPath, SerializedParam>,
    pointers: Vec<(ParamPath, String, Vec<ParamPath>)>,
) -> Result<Value, SubConfigError> {
    let mut json_val = serde_json::to_value(config_map).unwrap();
    let json_map: &mut serde_json::Map<std::string::String, serde_json::Value> =
        json_val.as_object_mut().unwrap();

    for (target_param, target_description, pointing_params_vec) in pointers {
        let first_pointing_serialized_param =
            get_serialized_param(pointing_params_vec.first().unwrap(), json_map)?;
        json_map.insert(
            target_param.clone(),
            json!(SerializedParam {
                description: target_description,
                value: first_pointing_serialized_param.value
            }),
        );

        for pointing_param in pointing_params_vec {
            let pointing_serialized_param = get_serialized_param(&pointing_param, json_map)?;
            json_map.remove(&pointing_param);
            json_map.insert(
                pointing_param,
                json!(PointerParam {
                    description: pointing_serialized_param.description,
                    pointer_target: target_param.to_owned()
                }),
            );
        }
    }
    Ok(json_val)
}

/// Separates a json map into config map of the raw values and pointers map.
pub fn get_maps_from_raw_json(
    json_map: Map<String, Value>,
) -> (BTreeMap<ParamPath, SerializedParam>, BTreeMap<ParamPath, ParamPath>) {
    let mut config_map: BTreeMap<String, SerializedParam> = BTreeMap::new();
    let mut pointers_map: BTreeMap<String, ParamPath> = BTreeMap::new();
    for (param_path, stored_param) in json_map {
        if let Ok(ser_param) = serde_json::from_value::<SerializedParam>(stored_param.clone()) {
            config_map.insert(param_path.to_owned(), ser_param);
        } else if let Ok(pointer_param) = serde_json::from_value::<PointerParam>(stored_param) {
            pointers_map.insert(param_path.to_owned(), pointer_param.pointer_target);
        } else {
            unreachable!("Invalid type in the json config map")
        }
    }
    (config_map, pointers_map)
}

/// Sets values in the config map to the params in the pointers map.
pub fn update_config_map_by_pointers(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    pointers_map: &BTreeMap<ParamPath, ParamPath>,
) -> Result<(), SubConfigError> {
    for (param_path, target_param_path) in pointers_map {
        let Some(serialized_param_target) = config_map.get(target_param_path) else {
            return Err(SubConfigError::PointerTargetNotFound { target_param: target_param_path.to_owned() });
        };
        config_map.insert(param_path.to_owned(), serialized_param_target.clone());
    }
    Ok(())
}

/// Updates the config map by param path to value custom json file.
pub fn update_config_map_by_custom_config(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    custom_config_path: &PathBuf,
) -> Result<(), SubConfigError> {
    let file = std::fs::File::open(custom_config_path).unwrap();
    let custom_config: Map<String, Value> = serde_json::from_reader(file).unwrap();
    for (param_path, json_value) in custom_config {
        update_config_map(config_map, param_path.as_str(), json_value)?;
    }
    Ok(())
}
