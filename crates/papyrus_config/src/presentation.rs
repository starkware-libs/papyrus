//! presentation of a configuration, with hiding or exposing private parameters.

use std::ops::IndexMut;

use itertools::Itertools;
use serde::Serialize;

use crate::dumping::SerializeConfig;
use crate::{ConfigError, ParamPrivacy};

/// Returns presentation of the public parameters in the config.
pub fn get_config_presentation<T: Serialize + SerializeConfig>(
    config: &T,
    include_private_parameters: bool,
) -> Result<serde_json::Value, ConfigError> {
    let mut config_presentation = serde_json::to_value(config)?;
    if include_private_parameters {
        return Ok(config_presentation);
    }

    // Iterates over flatten param paths for removing non-public parameters from the nested config.
    for (param_path, serialized_param) in config.dump() {
        match serialized_param.privacy {
            ParamPrivacy::Public => continue,
            ParamPrivacy::TemporaryValue => continue,
            ParamPrivacy::Private => remove_path_from_json(&param_path, &mut config_presentation)?,
        }
    }
    Ok(config_presentation)
}

// Gets a json in the format:
// {
//      a: {
//          b: {
//              v1: 1,
//              v2: 2
//          }
//      }
// }
// and a param path, for example 'a.b.v1', and removes the v1 from the json.
// The result will be:
// {
//      a: {
//          b: {
//              v2: 2
//          }
//      }
// }
fn remove_path_from_json(
    param_path: &str,
    json: &mut serde_json::Value,
) -> Result<(), ConfigError> {
    // given param_path = "a.b.v1", path_to_entry will be ["a", "b"] and entry_to_remove will
    // be "v1".
    let mut path_to_entry = param_path.split('.').collect_vec();
    let Some(entry_to_remove) = path_to_entry.pop() else {
        // TODO: Can we expect this to never happen?
        return Ok(()); // Empty param path.
    };

    // Traverse the json using path_to_entry to get to the part of the json that contains the entry
    // to remove, I.E. get json[a][b].
    let most_inner_json =
        path_to_entry.iter().fold(json, |entry, config_name| entry.index_mut(config_name));

    // Remove the entry from from the json.
    most_inner_json
        .as_object_mut()
        .ok_or_else(|| ConfigError::ParamNotFound { param_path: param_path.to_string() })?
        .remove(entry_to_remove);
    Ok(())
}
