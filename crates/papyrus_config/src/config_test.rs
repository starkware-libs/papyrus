use std::collections::BTreeMap;

use clap::Command;
use itertools::chain;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::command::update_config_map_by_command;
use crate::{
    append_sub_config_name, load, replace_pointers, ser_param, ParamPath, SerializeConfig,
    SerializedParam, POINTER_PREFIX,
};

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
    pub a: usize,
    pub b: String,
    pub c: bool,
}

impl SerializeConfig for TypicalConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param("a", &self.a, "This is a."),
            ser_param("b", &self.b, "This is b."),
            ser_param("c", &self.c, "This is c."),
        ])
    }
}

#[test]
fn test_update_dumped_config() {
    let command = Command::new("Testing");
    let mut dumped_config = TypicalConfig { a: 1, b: "bbb".to_owned(), c: false }.dump();
    let args = vec!["Testing", "--a", "1234", "--b", "15", "--c", "true"];

    update_config_map_by_command(&mut dumped_config, command, args).unwrap();

    assert_eq!(json!(1234), dumped_config["a"].value);
    assert_eq!(json!("15"), dumped_config["b"].value);
    assert_eq!(json!(true), dumped_config["c"].value);
}

#[test]
fn test_replace_pointers() {
    let map = BTreeMap::from([
        ser_param("a", &json!(5), "This is a."),
        ser_param("b", &json!(format!("{}a", POINTER_PREFIX)), "Pointer to a."),
        ser_param("c", &json!(format!("{}a", POINTER_PREFIX)), "Pointer to a."),
    ]);
    let new_map = replace_pointers(&map).unwrap();
    assert_eq!(new_map["a"], new_map["b"]);
    assert_eq!(new_map["a"], new_map["c"]);

    let invalid_map =
        BTreeMap::from([ser_param("b", &json!(format!("{}a", POINTER_PREFIX)), "Pointer to a.")]);
    assert!(replace_pointers(&invalid_map).is_err());
}
