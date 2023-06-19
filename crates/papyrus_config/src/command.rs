use std::collections::BTreeMap;

use clap::{Arg, ArgMatches, Command};
use serde_json::{json, Value};

use crate::{SerializedParam, SubConfigError};

pub type ParamPath = String;
pub type Description = String;

/// Takes the input for the command line interface and updates the config map.
/// Supports usize, bool and String.
pub fn update_config_map_by_command(
    config_map: &mut BTreeMap<ParamPath, SerializedParam>,
    command: Command,
    command_input: Vec<&str>,
) {
    let arg_match = command
        .args(get_args_parser(config_map))
        .try_get_matches_from(command_input)
        .unwrap_or_else(|e| e.exit());

    for param_path_id in arg_match.ids() {
        let param_path = param_path_id.as_str();
        let new_value = get_arg_by_type(config_map, &arg_match, param_path).unwrap();
        config_map.get_mut(param_path).unwrap().value = new_value;
    }
}

/// Determines the parser for the command input accordingly to the types of the current values.
fn get_args_parser(config_map: &BTreeMap<ParamPath, SerializedParam>) -> Vec<Arg> {
    let mut args_parser = Vec::new();
    for (param_path, serialized_param) in config_map.iter() {
        if let Value::Array(_) | Value::Object(_) | Value::Null = serialized_param.value {
            continue;
        }
        let arg = Arg::new(param_path)
            .long(param_path)
            .help(&serialized_param.description)
            .value_parser(match serialized_param.value {
                Value::Number(_) => clap::value_parser!(usize).into(),
                Value::Bool(_) => clap::value_parser!(bool),
                _ => clap::value_parser!(String),
            });
        args_parser.push(arg);
    }
    args_parser
}

/// Converts clap arg_matches into json values.
fn get_arg_by_type(
    config_map: &BTreeMap<ParamPath, SerializedParam>,
    arg_match: &ArgMatches,
    param_path: &str,
) -> Result<Value, SubConfigError> {
    match config_map[param_path].value {
        Value::Number(_) => Ok(json!(arg_match.try_get_one::<usize>(param_path)?)),
        Value::Bool(_) => Ok(json!(arg_match.try_get_one::<bool>(param_path)?)),
        Value::String(_) => Ok(json!(arg_match.try_get_one::<String>(param_path)?)),
        _ => panic!(),
    }
}
