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
    // For example, for the param path 'a.b.c.d', perform config_presentation[a][b][c].remove(d).
    for (param_path, serialized_param) in config.dump() {
        if let ParamPrivacy::Public = serialized_param.privacy {
            continue;
        }
        if let ParamPrivacy::TemporaryValue = serialized_param.privacy {
            continue;
        }

        // Remove a non-public parameter.
        let mut config_hierarchy = param_path.split('.').collect_vec();
        let Some(element_to_remove) = config_hierarchy.pop() else {
            continue; // Empty param path.`
        };
        let most_inner_config = config_hierarchy
            .iter()
            .fold(&mut config_presentation, |entry, config_name| entry.index_mut(config_name));

        most_inner_config
            .as_object_mut()
            .ok_or_else(|| ConfigError::ParamNotFound { param_path: param_path.to_string() })?
            .remove(element_to_remove);
    }
    Ok(config_presentation)
}
