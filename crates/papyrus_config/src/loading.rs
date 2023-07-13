//! Loads a configuration object, and set values for the fields in the following order of priority:
//! * Command line arguments.
//! * Environment variables (capital letters).
//! * Custom config file.
//! * Default config file.

use std::collections::BTreeMap;
use std::fs::File;
use std::mem::discriminant;
use std::ops::IndexMut;
use std::path::PathBuf;

use clap::Command;
use command::{get_command_matches, update_config_map_by_command_args};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{command, ParamPath, PointerParam, SerializedParam, SubConfigError};

pub fn load_and_process_config<T: for<'a> Deserialize<'a>>(
    default_config_file: File,
    command: Command,
    args: Vec<String>,
) -> Result<T, SubConfigError> {
    let deserialized_default_config: Map<String, Value> =
        serde_json::from_reader(default_config_file).unwrap();

    let (mut config_map, pointers_map) = get_maps_from_raw_json(deserialized_default_config);
    let arg_matches = get_command_matches(&config_map, command, args)?;
    if let Some(custom_config_path) = arg_matches.try_get_one::<PathBuf>("config_file")? {
        update_config_map_by_custom_config(&mut config_map, custom_config_path)?;
    };
    update_config_map_by_command_args(&mut config_map, &arg_matches)?;
    update_config_map_by_pointers(&mut config_map, &pointers_map)?;
    load(&config_map)
}

// Separates a json map into config map of the raw values and pointers map.
pub(crate) fn get_maps_from_raw_json(
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

/// Updates the config map by param path to value custom json file.
pub(crate) fn update_config_map_by_custom_config(
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

/// Sets values in the config map to the params in the pointers map.
pub(crate) fn update_config_map_by_pointers(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    pointers_map: &BTreeMap<ParamPath, ParamPath>,
) -> Result<(), SubConfigError> {
    for (param_path, target_param_path) in pointers_map {
        let Some(serialized_param_target) = config_map.get(target_param_path) else {
            return Err(SubConfigError::PointerTargetNotFound {
                target_param: target_param_path.to_owned(),
            });
        };
        config_map.insert(param_path.to_owned(), serialized_param_target.clone());
    }
    Ok(())
}

// Deserializes config from flatten JSON.
// For an explanation of `for<'a> Deserialize<'a>` see
// `<https://doc.rust-lang.org/nomicon/hrtb.html>`.
pub(crate) fn load<T: for<'a> Deserialize<'a>>(
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

pub(crate) fn update_config_map(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    param_path: &str,
    new_value: Value,
) -> Result<(), SubConfigError> {
    let Some(serialized_param) = config_map.get_mut(param_path) else {
        return Err(SubConfigError::ParamNotFound { param_path: param_path.to_string() });
    };
    if discriminant(&serialized_param.value) != discriminant(&new_value) {
        return Err(SubConfigError::ChangeParamType {
            param_path: param_path.to_string(),
            before: serialized_param.value.to_owned(),
            after: new_value,
        });
    }
    serialized_param.value = new_value;
    Ok(())
}
