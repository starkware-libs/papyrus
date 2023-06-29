use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::IndexMut;

use clap::parser::MatchesError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type ParamPath = String;
pub type Description = String;

pub mod command;
#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";
pub const POINTER_PREFIX: &str = "$";

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    pub description: String,
    pub value: Value,
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

pub fn update_config_map(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    param_path: &str,
    new_value: Value,
) -> Result<(), SubConfigError> {
    let Some(serialized_param) = config_map.get_mut(param_path) else {
        return Err(SubConfigError::ParamNotFound{param_path: param_path.to_string()});
    };
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

pub fn dump_to_file<T: SerializeConfig>(config: &T, file_path: &str) {
    let dumped = config.dump();
    let file = File::create(file_path).expect("creating failed");
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &dumped).expect("writing failed");
    writer.flush().expect("flushing failed");
}

/// Replaces param pointers (strings start with '$') with the values of the pointer target.
fn replace_pointer_param(
    config_map: &BTreeMap<ParamPath, SerializedParam>,
    serialized_param: &SerializedParam,
) -> Result<SerializedParam, SubConfigError> {
    let Some(str_path) = serialized_param.value.as_str() else {
        return Ok(serialized_param.clone());
    };
    let Some(path_target) = str_path.strip_prefix(POINTER_PREFIX)  else {
        return Ok(serialized_param.clone());
    };
    match config_map.get(path_target) {
        None => Err(SubConfigError::PointerTargetNotFound { target_param: str_path.to_string() }),
        Some(serialized_param_target) => Ok(serialized_param_target.clone()),
    }
}

/// Sets values for the pointer params in the config.
pub fn replace_pointers(
    config_map: &BTreeMap<ParamPath, SerializedParam>,
) -> Result<BTreeMap<ParamPath, SerializedParam>, SubConfigError> {
    let mut new_config_map = BTreeMap::<ParamPath, SerializedParam>::new();
    for (param_path, serialized_param) in config_map.iter() {
        new_config_map
            .insert(param_path.to_string(), replace_pointer_param(config_map, serialized_param)?);
    }
    Ok(new_config_map)
}
