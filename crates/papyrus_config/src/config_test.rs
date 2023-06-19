use std::collections::BTreeMap;

use itertools::chain;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    append_sub_config_name, ser_param, update_dumped_config, ParamPath, SerdeConfig,
    SerializedParam,
};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct InnerConfig {
    pub a: usize,
}

impl SerdeConfig for InnerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("a", &self.a, "This is a.")])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct OptionalConfig {
    pub o: usize,
}

impl SerdeConfig for OptionalConfig {
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

impl SerdeConfig for OuterConfig {
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
    let loaded_config = OuterConfig::load(&dump).unwrap();
    assert_eq!(loaded_config, outer_config);
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct TypicalConfig {
    pub a: usize,
    pub b: String,
    pub c: Option<bool>,
}

impl SerdeConfig for TypicalConfig {
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
    let dumped_config = TypicalConfig { a: 1, b: "2".to_owned(), c: Some(true) }.dump();
    let params_map_0 = BTreeMap::from([
        ("a".to_owned(), "1234".to_owned()),
        ("b".to_owned(), "/abc".to_owned()),
        ("c".to_owned(), "".to_owned()),
    ]);
    let config_map_0 = update_dumped_config(dumped_config, params_map_0).unwrap();

    assert_eq!(json!(1234), config_map_0["a"].value);
    assert_eq!(json!("/abc"), config_map_0["b"].value);
    assert!(config_map_0["c"].value.is_null());

    // Updating 'c' from null to bool.
    let params_map_1 = BTreeMap::from([("c".to_owned(), "true".to_owned())]);
    let config_map_1 = update_dumped_config(config_map_0, params_map_1);
    let config_map_1 = config_map_1.as_ref().unwrap();
    let c_value = &config_map_1.get("c").unwrap().value;
    assert!(c_value.as_bool().unwrap());
}
