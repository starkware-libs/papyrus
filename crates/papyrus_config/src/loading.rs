//! Loads a configuration object, and set values for the fields in the following order of priority:
//! * Command line arguments.
//! * Environment variables (capital letters).
//! * Custom config files, separated by ',' (comma), from last to first.
//! * Default config file.

use std::collections::BTreeMap;
use std::fs::File;
use std::ops::IndexMut;
use std::path::PathBuf;

use clap::parser::Values;
use clap::Command;
use command::{get_command_matches, update_config_map_by_command_args};
use itertools::any;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::validators::validate_path_exists;
use crate::{
    command,
    ConfigError,
    Description,
    ParamPath,
    SerializationType,
    SerializedContent,
    SerializedParam,
    IS_NONE_MARK,
};

/// Deserializes config from flatten JSON.
/// For an explanation of `for<'a> Deserialize<'a>` see
/// `<https://doc.rust-lang.org/nomicon/hrtb.html>`.
pub fn load<T: for<'a> Deserialize<'a>>(
    config_map: &BTreeMap<ParamPath, Value>,
) -> Result<T, ConfigError> {
    let mut nested_map = json!({});
    for (param_path, value) in config_map {
        let mut entry = &mut nested_map;
        for config_name in param_path.split('.') {
            entry = entry.index_mut(config_name);
        }
        *entry = value.clone();
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
        serde_json::from_reader(default_config_file)?;
    let default_config_map = convert_json_map_to_config_map(deserialized_default_config);
    inner_load_and_process_config(default_config_map, command, args)
}

#[derive(Debug, Default)]
pub(crate) struct ProcessingMaps {
    // Intermediate values of the params.
    pub values: BTreeMap<ParamPath, Value>,
    // Description of the params.
    pub descriptions: BTreeMap<ParamPath, Description>,
    // Type of the params.
    pub types: BTreeMap<ParamPath, SerializationType>,
    // Source and target of the pointers.
    pub pointers: BTreeMap<ParamPath, ParamPath>,
}

impl TryFrom<BTreeMap<String, SerializedParam>> for ProcessingMaps {
    type Error = ConfigError;

    fn try_from(config_map: BTreeMap<String, SerializedParam>) -> Result<Self, Self::Error> {
        let mut processing_maps = ProcessingMaps::default();

        for (param_path, ser_param) in config_map {
            match &ser_param.content {
                // Insert values.
                SerializedContent::DefaultValue(value) => {
                    processing_maps.values.insert(param_path.to_owned(), value.to_owned());
                }
                // Insert pointers.
                SerializedContent::PointerTarget(pointer_target_param) => {
                    processing_maps
                        .pointers
                        .insert(param_path.to_owned(), pointer_target_param.to_owned());
                }
                SerializedContent::ParamType(_) => {}
            };

            // Insert types, apart from the pointers type.
            if let Some(serialization_type) = ser_param.content.get_serialization_type() {
                processing_maps.types.insert(param_path.to_owned(), serialization_type);
            }
            // Insert descriptions.
            processing_maps.descriptions.insert(param_path, ser_param.description);
        }

        // Insert types of the pointers.
        for (param_path, target_param_path) in processing_maps.pointers.iter() {
            let target_type = get_pointer_target_value(&processing_maps.types, target_param_path)?;
            processing_maps.types.insert(param_path.to_string(), target_type.to_owned());
        }

        Ok(processing_maps)
    }
}

// Updates the default config map.
pub(crate) fn inner_load_and_process_config<T: for<'a> Deserialize<'a>>(
    default_config_map: BTreeMap<String, SerializedParam>,
    command: Command,
    args: Vec<String>,
) -> Result<T, ConfigError> {
    let mut processing_maps: ProcessingMaps = default_config_map.try_into()?;
    // Take param paths with corresponding descriptions, and get the matching arguments.
    let mut arg_matches =
        get_command_matches(&processing_maps.types, &processing_maps.descriptions, command, args)?;
    // If the config_file arg is given, updates the values map according to this files.
    if let Some(custom_config_paths) = arg_matches.remove_many::<PathBuf>("config_file") {
        update_config_map_by_custom_configs(
            &mut processing_maps.values,
            &processing_maps.types,
            custom_config_paths,
        )?;
    };
    // Updates the values map according to the args.
    update_config_map_by_command_args(
        &mut processing_maps.values,
        &processing_maps.types,
        &arg_matches,
    )?;
    // Set values to the pointers.
    update_config_map_by_pointers(&mut processing_maps.values, &processing_maps.pointers)?;
    // Set values according to the is-none marks.
    update_optional_values(&mut processing_maps.values);
    // Build and return a Config object.
    load(&processing_maps.values)
}

fn convert_json_map_to_config_map(
    json_map: Map<String, Value>,
) -> BTreeMap<ParamPath, SerializedParam> {
    json_map
        .into_iter()
        .map(|(param_path, stored_param)| {
            let Ok(ser_param) = serde_json::from_value::<SerializedParam>(stored_param.clone())
            else {
                unreachable!("Invalid type in the json config map")
            };
            (param_path, ser_param)
        })
        .collect()
}

fn get_pointer_target_value<'a, T>(
    config_map: &'a BTreeMap<ParamPath, T>,
    target_param_path: &String,
) -> Result<&'a T, ConfigError> {
    match config_map.get(target_param_path) {
        Some(target_value) => Ok(target_value),
        None => {
            Err(ConfigError::PointerTargetNotFound { target_param: target_param_path.to_owned() })
        }
    }
}

// Updates the config map by param path to value custom json files.
pub(crate) fn update_config_map_by_custom_configs(
    config_map: &mut BTreeMap<ParamPath, Value>,
    types_map: &BTreeMap<ParamPath, SerializationType>,
    custom_config_paths: Values<PathBuf>,
) -> Result<(), ConfigError> {
    for config_path in custom_config_paths {
        validate_path_exists(&config_path)?;
        let file = std::fs::File::open(config_path)?;
        let custom_config: Map<String, Value> = serde_json::from_reader(file)?;
        for (param_path, json_value) in custom_config {
            update_config_map(config_map, types_map, param_path.as_str(), json_value)?;
        }
    }
    Ok(())
}

// Sets values in the config map to the params in the pointers map that have not yet been assigned a
// value.
pub(crate) fn update_config_map_by_pointers(
    config_map: &mut BTreeMap<ParamPath, Value>,
    pointers_map: &BTreeMap<ParamPath, ParamPath>,
) -> Result<(), ConfigError> {
    for (param_path, target_param_path) in pointers_map {
        if config_map.get(param_path).is_some() {
            continue;
        }
        let target_value = get_pointer_target_value(config_map, target_param_path)?;
        config_map.insert(param_path.to_owned(), target_value.clone());
    }
    Ok(())
}

// Removes the none marks, and sets null for the params marked as None instead of the inner params.
fn update_optional_values(config_map: &mut BTreeMap<ParamPath, Value>) {
    let optional_params: Vec<_> = config_map
        .keys()
        .filter_map(|param_path| param_path.strip_suffix(&format!(".{IS_NONE_MARK}")))
        .map(|param_path| param_path.to_owned())
        .collect();
    let mut none_params = vec![];
    for optional_param in optional_params {
        let value = config_map
            .remove(&format!("{optional_param}.{IS_NONE_MARK}"))
            .expect("Not found optional param");
        if value == json!(true) {
            none_params.push(optional_param);
        }
    }
    // Remove param paths that start with any None param.
    config_map.retain(|param_path, _| {
        !any(&none_params, |none_param| param_path.starts_with(none_param))
    });
    for none_param in none_params {
        config_map.insert(none_param, Value::Null);
    }
}

pub(crate) fn update_config_map(
    config_map: &mut BTreeMap<ParamPath, Value>,
    types_map: &BTreeMap<ParamPath, SerializationType>,
    param_path: &str,
    new_value: Value,
) -> Result<(), ConfigError> {
    let Some(serialization_type) = types_map.get(param_path) else {
        return Err(ConfigError::ParamNotFound { param_path: param_path.to_string() });
    };
    let is_type_matched = match serialization_type {
        SerializationType::Number => new_value.is_number(),
        SerializationType::Boolean => new_value.is_boolean(),
        SerializationType::String => new_value.is_string(),
    };
    if !is_type_matched {
        return Err(ConfigError::ChangeRequiredParamType {
            param_path: param_path.to_string(),
            required: serialization_type.to_owned(),
            given: new_value,
        });
    }

    config_map.insert(param_path.to_owned(), new_value);
    Ok(())
}
