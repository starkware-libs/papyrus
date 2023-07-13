use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::{ParamPath, PointerParam, SerializedParam, SubConfigError};

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";

/// Serialization for configs.
pub trait SerializeConfig {
    /// Conversion of a configuration to a mapping of flattened parameters to their descriptions and
    /// values.
    /// Note, in the case of a None sub configs, its elements will not included in the flatten map.
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam>;
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
