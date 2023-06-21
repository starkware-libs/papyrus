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
pub struct OuterConfig {
    pub inner: InnerConfig,
    pub b: usize,
}

impl SerdeConfig for OuterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([(
            String::from("b"),
            SerializedParam { description: String::from("This is b."), value: json!(self.b) },
        )])
        .into_iter()
        .chain(append_sub_config_name(self.inner.dump(), "inner"))
        .collect()
    }
}

#[test]
fn dump_and_load_config() {
    let outer_config = OuterConfig { b: 1, inner: InnerConfig { a: 0 } };
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
    ]);
    assert_eq!(dump, expected);
    let loaded_config = OuterConfig::load(&dump).unwrap();
    assert_eq!(loaded_config, outer_config);
}
