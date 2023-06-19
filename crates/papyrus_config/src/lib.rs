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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    pub description: String,
    pub value: Value,
}

#[derive(thiserror::Error, Debug)]
pub enum SubConfigError {
    #[error(transparent)]
    MissingParam(#[from] serde_json::Error),
    #[error(transparent)]
    Matches(#[from] MatchesError),
}
/// Serialization and deserialization for configs.
/// For an explanation of `for<'a> Deserialize<'a>` see
/// `<https://doc.rust-lang.org/nomicon/hrtb.html>`.
pub trait SerdeConfig: for<'a> Deserialize<'a> + Serialize + Sized {
    /// Serializes the config into flatten JSON.
    /// Note, in the case of a None sub configs, its elements will not included in the flatten map.
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam>;

    /// Deserializes the config from flatten JSON.
    fn load(config_dump: &BTreeMap<ParamPath, SerializedParam>) -> Result<Self, SubConfigError> {
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

pub fn dump_to_file<T: SerdeConfig>(config: &T, file_path: &str) {
    let dumped = config.dump();
    let file = File::create(file_path).expect("creating failed");
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &dumped).expect("writing failed");
    writer.flush().expect("flushing failed");
}
