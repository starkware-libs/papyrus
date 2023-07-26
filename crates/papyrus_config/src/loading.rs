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

use crate::{command, ConfigError, ParamPath, PointerParam, SerializedParam, IS_NONE_MARK};

/// Deserializes config from flatten JSON.
/// For an explanation of `for<'a> Deserialize<'a>` see
/// `<https://doc.rust-lang.org/nomicon/hrtb.html>`.
pub fn load<T: for<'a> Deserialize<'a>>(
    config_dump: &BTreeMap<ParamPath, SerializedParam>,
) -> Result<T, ConfigError> {
    let mut nested_map = json!({});
    // Iterates in reverse order, to give higher priority to higher entries in the nested map.
    for (param_path, serialized_param) in config_dump.iter().rev() {
        let mut entry = &mut nested_map;
        for config_name in param_path.split('.') {
            entry = entry.index_mut(config_name);
        }
        *entry = serialized_param.value.clone();
    }
    Ok(serde_json::from_value(nested_map)?)
}

/// Deserializes a json config file, updates the values by the given arguments for the command, and
/// set values for the pointers.
pub fn load_and_process_config<T: for<'a> Deserialize<'a>>(
    default_config_file: File,
    command: Command,
    args: Vec<String>,
) -> Result<T, ConfigError> {
    let deserialized_default_config: Map<String, Value> =
        serde_json::from_reader(default_config_file).unwrap();

    let (mut config_map, pointers_map) = get_maps_from_raw_json(deserialized_default_config);
    let arg_matches = get_command_matches(&config_map, command, args)?;
    if let Some(custom_config_path) = arg_matches.try_get_one::<PathBuf>("config_file")? {
        update_config_map_by_custom_config(&mut config_map, custom_config_path)?;
    };
    update_config_map_by_command_args(&mut config_map, &arg_matches)?;
    update_config_map_by_pointers(&mut config_map, &pointers_map)?;
    update_optional_values(&mut config_map);
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

// Updates the config map by param path to value custom json file.
pub(crate) fn update_config_map_by_custom_config(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    custom_config_path: &PathBuf,
) -> Result<(), ConfigError> {
    let file = std::fs::File::open(custom_config_path).unwrap();
    let custom_config: Map<String, Value> = serde_json::from_reader(file).unwrap();
    for (param_path, json_value) in custom_config {
        update_config_map(config_map, param_path.as_str(), json_value)?;
    }
    Ok(())
}

// Sets values in the config map to the params in the pointers map.
pub(crate) fn update_config_map_by_pointers(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    pointers_map: &BTreeMap<ParamPath, ParamPath>,
) -> Result<(), ConfigError> {
    for (param_path, target_param_path) in pointers_map {
        let Some(serialized_param_target) = config_map.get(target_param_path) else {
            return Err(ConfigError::PointerTargetNotFound {
                target_param: target_param_path.to_owned(),
            });
        };
        config_map.insert(param_path.to_owned(), serialized_param_target.clone());
    }
    Ok(())
}

// Removes the none marks, and sets null for the params marked as None.
pub(crate) fn update_optional_values(config_map: &mut BTreeMap<ParamPath, SerializedParam>) {
    let optional_params: Vec<_> = config_map
        .keys()
        .filter(|param_path| param_path.ends_with(&format!(".{}", IS_NONE_MARK)))
        .map(|param_path| param_path.to_owned())
        .collect();
    for optional_param in optional_params {
        let serialized_optional_param = config_map.remove(&optional_param).unwrap();
        if serialized_optional_param.value == json!(true) {
            config_map.insert(
                optional_param.strip_suffix(&format!(".{}", IS_NONE_MARK)).unwrap().to_owned(),
                SerializedParam {
                    description: serialized_optional_param.description,
                    value: Value::Null,
                },
            );
        }
    }
}

pub(crate) fn update_config_map(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    param_path: &str,
    new_value: Value,
) -> Result<(), ConfigError> {
    let Some(serialized_param) = config_map.get_mut(param_path) else {
        return Err(ConfigError::ParamNotFound { param_path: param_path.to_string() });
    };
    if discriminant(&serialized_param.value) != discriminant(&new_value) {
        return Err(ConfigError::ChangeParamType {
            param_path: param_path.to_string(),
            before: serialized_param.value.to_owned(),
            after: new_value,
        });
    }
    serialized_param.value = new_value;
    Ok(())
}
