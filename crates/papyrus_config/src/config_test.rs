use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use assert_matches::assert_matches;
use clap::Command;
use itertools::chain;
use serde::{Deserialize, Serialize};
use serde_json::json;
use test_utils::get_absolute_path;

use crate::command::{get_command_matches, update_config_map_by_command_args};
use crate::converters::deserialize_milliseconds_to_duration;
use crate::dumping::{
    append_sub_config_name, combine_config_map_and_pointers, ser_optional_param,
    ser_optional_sub_config, ser_param, SerializeConfig,
};
use crate::loading::{
    get_maps_from_raw_json, load, remove_description, update_config_map_by_custom_config,
    update_config_map_by_pointers, update_optional_values,
};
use crate::{ConfigError, ParamPath, PointerParam, SerializedParam};

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
        let mut dumped = remove_description(outer_config.dump());
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
    let dumped_config =
        TypicalConfig { a: Duration::from_secs(1), b: "bbb".to_owned(), c: false }.dump();
    let args = vec!["Testing", "--a", "1234", "--b", "15"];
    env::set_var("C", "true");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();

    let arg_matches = get_command_matches(&dumped_config, command, args).unwrap();
    let mut config_map = remove_description(dumped_config);
    update_config_map_by_command_args(&mut config_map, &arg_matches).unwrap();

    assert_eq!(json!(1234), config_map["a"]);
    assert_eq!(json!("15"), config_map["b"]);
    assert_eq!(json!(true), config_map["c"]);

    let loaded_config: TypicalConfig = load(&config_map).unwrap();
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
    let (loaded_config_map, loaded_pointers_map) = get_maps_from_raw_json(loaded);
    let mut config_map = remove_description(loaded_config_map);
    update_config_map_by_pointers(&mut config_map, &loaded_pointers_map).unwrap();
    assert_eq!(config_map["a1"], json!(10));
    assert_eq!(config_map["a1"], config_map["a2"]);
}

#[test]
fn test_replace_pointers() {
    let mut config_map =
        remove_description(BTreeMap::from([ser_param("a", &json!(5), "This is a.")]));
    let pointers_map =
        BTreeMap::from([("b".to_owned(), "a".to_owned()), ("c".to_owned(), "a".to_owned())]);
    update_config_map_by_pointers(&mut config_map, &pointers_map).unwrap();
    assert_eq!(config_map["a"], config_map["b"]);
    assert_eq!(config_map["a"], config_map["c"]);

    let err = update_config_map_by_pointers(&mut BTreeMap::default(), &pointers_map).unwrap_err();
    assert_matches!(err, ConfigError::PointerTargetNotFound { .. });
}

#[test]
fn test_update_by_custom_config() {
    let mut config_map = remove_description(BTreeMap::from([ser_param(
        "param_path",
        &json!("default value"),
        "This is a.",
    )]));
    let custom_config_path =
        get_absolute_path("crates/papyrus_config/resources/custom_config_example.json");
    update_config_map_by_custom_config(&mut config_map, &custom_config_path).unwrap();
    assert_eq!(config_map["param_path"], json!("custom value"));
}
