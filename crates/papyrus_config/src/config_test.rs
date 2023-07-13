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
    append_sub_config_name, combine_config_map_and_pointers, ser_param, SerializeConfig,
};
use crate::loading::{
    get_maps_from_raw_json, load, update_config_map_by_custom_config, update_config_map_by_pointers,
};
use crate::{ParamPath, PointerParam, SerializedParam, SubConfigError};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct InnerConfig {
    pub a: usize,
}

impl SerializeConfig for InnerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("a", &self.a, "This is a.")])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct OptionalConfig {
    pub o: usize,
}

impl SerializeConfig for OptionalConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("o", &self.o, "This is o.")])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct OuterConfig {
    pub inner: InnerConfig,
    pub b: usize,
    pub some_optional: Option<OptionalConfig>,
    pub none_optional: Option<OptionalConfig>,
}

impl SerializeConfig for OuterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        chain!(
            BTreeMap::from([ser_param("b", &self.b, "This is b.")]),
            append_sub_config_name(self.inner.dump(), "inner"),
            match &self.some_optional {
                None => BTreeMap::new(),
                Some(optional_config) => {
                    append_sub_config_name(optional_config.dump(), "some_optional")
                }
            },
            match &self.none_optional {
                None => BTreeMap::new(),
                Some(optional_config) => {
                    append_sub_config_name(optional_config.dump(), "none_optional")
                }
            },
        )
        .collect()
    }
}

#[test]
fn dump_and_load_config() {
    let optional_config = OptionalConfig { o: 2 };
    let outer_config = OuterConfig {
        b: 1,
        inner: InnerConfig { a: 0 },
        some_optional: { Some(optional_config) },
        none_optional: None,
    };
    let dump = outer_config.dump();
    let expected = BTreeMap::from([
        (
            "inner.a".to_owned(),
            SerializedParam { description: "This is a.".to_owned(), value: json!(0) },
        ),
        ("b".to_owned(), SerializedParam { description: "This is b.".to_owned(), value: json!(1) }),
        (
            "some_optional.o".to_owned(),
            SerializedParam { description: "This is o.".to_owned(), value: json!(2) },
        ),
    ]);
    assert_eq!(dump, expected);
    let loaded_config = load::<OuterConfig>(&dump).unwrap();
    assert_eq!(loaded_config, outer_config);
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct TypicalConfig {
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub a: Duration,
    pub b: String,
    pub c: bool,
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
        "common_a".to_owned(),
        "This is common a".to_owned(),
        vec!["a1".to_owned(), "a2".to_owned()],
    )];
    let stored_map = combine_config_map_and_pointers(config_map, pointers).unwrap();
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
        json!(SerializedParam { description: "This is common a".to_owned(), value: json!(5) })
    );

    let serialized = serde_json::to_string(&stored_map).unwrap();
    let loaded = serde_json::from_str(&serialized).unwrap();
    let (mut loaded_config_map, loaded_pointers_map) = get_maps_from_raw_json(loaded);
    update_config_map_by_pointers(&mut loaded_config_map, &loaded_pointers_map).unwrap();
    assert_eq!(loaded_config_map["a1"].value, json!(5));
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
    assert_matches!(err, SubConfigError::PointerTargetNotFound { .. });
}

#[test]
pub fn test_update_by_custom_config() {
    let mut config_map =
        BTreeMap::from([ser_param("param_path", &json!("default value"), "This is a.")]);
    let custom_config_path =
        get_absolute_path("crates/papyrus_config/resources/custom_config_example.json");
    update_config_map_by_custom_config(&mut config_map, &custom_config_path).unwrap();
    assert_eq!(config_map["param_path"].value, json!("custom value"));
}
