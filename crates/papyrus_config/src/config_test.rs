use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{append_sub_config_name, ParamPath, SerdeConfig, SerializedParam};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct InnerConfig {
    pub a: usize,
}

impl SerdeConfig for InnerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([(
            String::from("a"),
            SerializedParam { description: String::from("This is a."), value: json!(self.a) },
        )])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct OptionalConfig {
    pub o: usize,
}

impl SerdeConfig for OptionalConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([(
            String::from("o"),
            SerializedParam { description: String::from("This is o."), value: json!(self.o) },
        )])
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
        BTreeMap::from([(
            String::from("b"),
            SerializedParam { description: String::from("This is b."), value: json!(self.b) },
        )])
        .into_iter()
        .chain(append_sub_config_name(self.inner.dump(), "inner"))
        .chain(match &self.some_optional {
            None => BTreeMap::new(),
            Some(optional_config) => {
                append_sub_config_name(optional_config.dump(), "some_optional")
            }
        })
        .chain(match &self.none_optional {
            None => BTreeMap::new(),
            Some(optional_config) => {
                append_sub_config_name(optional_config.dump(), "none_optional")
            }
        })
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
            String::from("inner.a"),
            SerializedParam { description: String::from("This is a."), value: json!(0) },
        ),
        (
            String::from("b"),
            SerializedParam { description: String::from("This is b."), value: json!(1) },
        ),
        (
            String::from("some_optional.o"),
            SerializedParam { description: String::from("This is o."), value: json!(2) },
        ),
    ]);
    assert_eq!(dump, expected);
    let loaded_config = OuterConfig::load(&dump).unwrap();
    assert_eq!(loaded_config, outer_config);
}
