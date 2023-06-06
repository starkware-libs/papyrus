use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{ParamPath, SerdeConfig, SerializedParam};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct InnerConfig {
    pub a: String,
    pub b: usize,
}

impl SerdeConfig for InnerConfig {
    fn config_name() -> String {
        String::from("inner")
    }

    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            (
                String::from("a"),
                SerializedParam { description: String::from("This is a."), value: json!(self.a) },
            ),
            (
                String::from("b"),
                SerializedParam { description: String::from("This is b."), value: json!(self.b) },
            ),
        ])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct OuterConfig {
    pub inner: InnerConfig,
    pub c: usize,
}

impl SerdeConfig for OuterConfig {
    fn config_name() -> String {
        String::from("outer")
    }

    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([(
            String::from("c"),
            SerializedParam { description: String::from("This is c."), value: json!(self.c) },
        )])
        .into_iter()
        .chain(self.inner.dump_sub_config())
        .collect()
    }
}

#[test]
fn dump_and_load_config() {
    let outer_config = OuterConfig { c: 0, inner: InnerConfig { a: String::from("1"), b: 2 } };

    let loaded_sub_config = OuterConfig::load_sub_config(&outer_config.dump_sub_config()).unwrap();
    assert_eq!(loaded_sub_config, outer_config);
    let loaded_config = OuterConfig::load(&outer_config.dump()).unwrap();
    assert_eq!(loaded_config, outer_config);
}
