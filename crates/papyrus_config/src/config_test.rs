use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use assert_matches::assert_matches;
use clap::Command;
use itertools::chain;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::TempDir;
use test_utils::get_absolute_path;

use crate::command::{get_command_matches, update_config_map_by_command_args};
use crate::converters::deserialize_milliseconds_to_duration;
use crate::dumping::{
    append_sub_config_name,
    combine_config_map_and_pointers,
    ser_optional_param,
    ser_optional_sub_config,
    ser_param,
    SerializeConfig,
};
use crate::loading::{
    get_maps_from_raw_json,
    load,
    load_and_process_config,
    update_config_map_by_pointers,
    update_optional_values,
};
use crate::{ConfigError, ParamPath, PointerParam, SerializedParam};

lazy_static! {
    static ref CUSTOM_CONFIG_PATH: PathBuf =
        get_absolute_path("crates/papyrus_config/resources/custom_config_example.json");
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq)]
struct InnerConfig {
    o: usize,
}

impl SerializeConfig for InnerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("o", &self.o, "This is o.")])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct OuterConfig {
    opt_elem: Option<usize>,
    opt_config: Option<InnerConfig>,
    inner_config: InnerConfig,
}

impl SerializeConfig for OuterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        chain!(
            ser_optional_param(&self.opt_elem, 1, "opt_elem", "This is elem."),
            ser_optional_sub_config(&self.opt_config, "opt_config"),
            append_sub_config_name(self.inner_config.dump(), "inner_config"),
        )
        .collect()
    }
}

#[test]
fn dump_and_load_config() {
    let some_outer_config = OuterConfig {
        opt_elem: Some(2),
        opt_config: Some(InnerConfig { o: 3 }),
        inner_config: InnerConfig { o: 4 },
    };
    let none_outer_config =
        OuterConfig { opt_elem: None, opt_config: None, inner_config: InnerConfig { o: 5 } };

    for outer_config in [some_outer_config, none_outer_config] {
        let mut dumped = outer_config.dump();
        update_optional_values(&mut dumped);
        let loaded_config = load::<OuterConfig>(&dumped).unwrap();
        assert_eq!(loaded_config, outer_config);
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct TypicalConfig {
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    a: Duration,
    b: String,
    c: bool,
}

impl SerializeConfig for TypicalConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param("a", &self.a.as_millis(), "This is a as milliseconds."),
            ser_param("b", &self.b, "This is b."),
            ser_param("c", &self.c, "This is c."),
        ])
    }
}

#[test]
fn test_update_dumped_config() {
    let command = Command::new("Testing");
    let mut dumped_config =
        TypicalConfig { a: Duration::from_secs(1), b: "bbb".to_owned(), c: false }.dump();
    let args = vec!["Testing", "--a", "1234", "--b", "15"];
    env::set_var("C", "true");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();

    let arg_matches = get_command_matches(&dumped_config, command, args).unwrap();
    update_config_map_by_command_args(&mut dumped_config, &arg_matches).unwrap();

    assert_eq!(json!(1234), dumped_config["a"].value);
    assert_eq!(json!("15"), dumped_config["b"].value);
    assert_eq!(json!(true), dumped_config["c"].value);

    let loaded_config: TypicalConfig = load(&dumped_config).unwrap();
    assert_eq!(Duration::from_millis(1234), loaded_config.a);
}

#[test]
fn test_pointers_flow() {
    let config_map = BTreeMap::from([
        ser_param("a1", &json!(5), "This is a."),
        ser_param("a2", &json!(5), "This is a."),
    ]);
    let pointers = vec![(
        ser_param("common_a", &json!(10), "This is common a"),
        vec!["a1".to_owned(), "a2".to_owned()],
    )];
    let stored_map = combine_config_map_and_pointers(config_map, &pointers).unwrap();
    assert_eq!(
        stored_map["a1"],
        json!(PointerParam {
            description: "This is a.".to_owned(),
            pointer_target: "common_a".to_owned()
        })
    );
    assert_eq!(stored_map["a2"], stored_map["a1"]);
    assert_eq!(
        stored_map["common_a"],
        json!(SerializedParam { description: "This is common a".to_owned(), value: json!(10) })
    );

    let serialized = serde_json::to_string(&stored_map).unwrap();
    let loaded = serde_json::from_str(&serialized).unwrap();
    let (mut loaded_config_map, loaded_pointers_map) = get_maps_from_raw_json(loaded);
    update_config_map_by_pointers(&mut loaded_config_map, &loaded_pointers_map).unwrap();
    assert_eq!(loaded_config_map["a1"].value, json!(10));
    assert_eq!(loaded_config_map["a1"], loaded_config_map["a2"]);
}

#[test]
fn test_replace_pointers() {
    let mut config_map = BTreeMap::from([ser_param("a", &json!(5), "This is a.")]);
    let pointers_map =
        BTreeMap::from([("b".to_owned(), "a".to_owned()), ("c".to_owned(), "a".to_owned())]);
    update_config_map_by_pointers(&mut config_map, &pointers_map).unwrap();
    assert_eq!(config_map["a"], config_map["b"]);
    assert_eq!(config_map["a"], config_map["c"]);

    let err = update_config_map_by_pointers(&mut BTreeMap::default(), &pointers_map).unwrap_err();
    assert_matches!(err, ConfigError::PointerTargetNotFound { .. });
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
struct CustomConfig {
    param_path: String,
}

impl SerializeConfig for CustomConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("param_path", &self.param_path, "This is param_path.")])
    }
}

// Loads param_path of CustomConfig from args.
fn load_param_path(args: Vec<&str>) -> String {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config.json");
    CustomConfig { param_path: "default value".to_owned() }
        .dump_to_file(&vec![], file_path.to_str().unwrap())
        .unwrap();

    let loaded_config = load_and_process_config::<CustomConfig>(
        File::open(file_path).unwrap(),
        Command::new("Program"),
        args.into_iter().map(|s| s.to_owned()).collect(),
    )
    .unwrap();
    loaded_config.param_path
}

#[test]
fn test_load_default_config() {
    let args = vec!["Testing"];
    let param_path = load_param_path(args);
    assert_eq!(param_path, "default value");
}

#[test]
fn test_load_custom_config_file() {
    let args = vec!["Testing", "-f", CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_param_path(args);
    assert_eq!(param_path, "custom value");
}

#[test]
fn test_load_custom_config_file_and_args() {
    let args = vec![
        "Testing",
        "--config_file",
        CUSTOM_CONFIG_PATH.to_str().unwrap(),
        "--param_path",
        "command value",
    ];
    let param_path = load_param_path(args);
    assert_eq!(param_path, "command value");
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
