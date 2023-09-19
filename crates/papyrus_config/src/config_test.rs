use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

use assert_matches::assert_matches;
use clap::Command;
use itertools::chain;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use test_utils::get_absolute_path;
use validator::Validate;

use crate::converters::deserialize_milliseconds_to_duration;
use crate::dumping::{
    append_sub_config_name,
    combine_config_map_and_pointers,
    ser_generated_param,
    ser_optional_param,
    ser_optional_sub_config,
    ser_param,
    ser_pointer_target_param,
    ser_required_param,
    SerializeConfig,
};
use crate::loading::{
    inner_load_and_process_config,
    load,
    split_config_map,
    update_config_map_by_pointers,
};
use crate::presentation::get_config_presentation;
use crate::{
    ConfigError,
    ParamPath,
    ParamPrivacy,
    ParamPrivacyInput,
    SerializationType,
    SerializedContent,
    SerializedParam,
};

lazy_static! {
    static ref CUSTOM_CONFIG_PATH: PathBuf =
        get_absolute_path("crates/papyrus_config/resources/custom_config_example.json");
}

fn load_and_process_util<T: for<'a> Deserialize<'a>>(
    default_config_map: BTreeMap<String, SerializedParam>,
    mut args: Vec<String>,
) -> Result<T, ConfigError> {
    args.insert(0, "Testing".to_owned());
    inner_load_and_process_config(default_config_map, Command::new("Testing"), args)
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Validate)]
struct InnerConfig {
    #[validate(range(min = 0, max = 10))]
    o: usize,
}

impl SerializeConfig for InnerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param("o", &self.o, "This is o.", ParamPrivacyInput::Public)])
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Validate)]
struct OuterConfig {
    opt_elem: Option<usize>,
    opt_config: Option<InnerConfig>,
    #[validate]
    inner_config: InnerConfig,
}

impl SerializeConfig for OuterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        chain!(
            ser_optional_param(
                &self.opt_elem,
                1,
                "opt_elem",
                "This is elem.",
                ParamPrivacyInput::Public
            ),
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
        let dumped = load_and_process_util(outer_config.dump(), vec![]).unwrap();
        let loaded_config = load::<OuterConfig>(&dumped).unwrap();
        assert_eq!(loaded_config, outer_config);
    }
}

#[test]
fn test_validation() {
    let outer_config =
        OuterConfig { opt_elem: None, opt_config: None, inner_config: InnerConfig { o: 20 } };
    assert!(outer_config.validate().is_err());
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
            ser_param(
                "a",
                &self.a.as_millis(),
                "This is a as milliseconds.",
                ParamPrivacyInput::Public,
            ),
            ser_param("b", &self.b, "This is b.", ParamPrivacyInput::Public),
            ser_param("c", &self.c, "This is c.", ParamPrivacyInput::Private),
        ])
    }
}

#[test]
fn test_update_dumped_config() {
    let dumped_config =
        TypicalConfig { a: Duration::from_secs(1), b: "bbb".to_owned(), c: false }.dump();
    let args = vec!["--a", "1234", "--b", "15"];
    env::set_var("C", "true");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();
    let loaded_config: TypicalConfig = load_and_process_util(dumped_config, args).unwrap();

    assert_eq!(Duration::from_millis(1234), loaded_config.a);
    assert_eq!("15", loaded_config.b);
    assert!(loaded_config.c);
}

#[test]
fn test_config_presentation() {
    let config = TypicalConfig { a: Duration::from_secs(1), b: "bbb".to_owned(), c: false };
    let presentation = get_config_presentation(&config, true).unwrap();
    let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["a", "b", "c"]);

    let public_presentation = get_config_presentation(&config, false).unwrap();
    let keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["a", "b"]);
}

#[test]
fn test_pointers_flow() {
    let config_map = BTreeMap::from([
        ser_param("a1", &json!(5), "This is a.", ParamPrivacyInput::Public),
        ser_param("a2", &json!(5), "This is a.", ParamPrivacyInput::Private),
    ]);
    let pointers = vec![(
        ser_pointer_target_param("common_a", &json!(10), "This is common a"),
        vec!["a1".to_owned(), "a2".to_owned()],
    )];
    let stored_map = combine_config_map_and_pointers(config_map, &pointers).unwrap();
    assert_eq!(
        stored_map["a1"],
        json!(SerializedParam {
            description: "This is a.".to_owned(),
            content: SerializedContent::PointerTarget("common_a".to_owned()),
            privacy: ParamPrivacy::Public,
        })
    );
    assert_eq!(
        stored_map["a2"],
        json!(SerializedParam {
            description: "This is a.".to_owned(),
            content: SerializedContent::PointerTarget("common_a".to_owned()),
            privacy: ParamPrivacy::Private,
        })
    );
    assert_eq!(
        stored_map["common_a"],
        json!(SerializedParam {
            description: "This is common a".to_owned(),
            content: SerializedContent::DefaultValue(json!(10)),
            privacy: ParamPrivacy::TemporaryValue,
        })
    );

    let serialized = serde_json::to_string(&stored_map).unwrap();
    let loaded = serde_json::from_str(&serialized).unwrap();
    let (mut values_map, _, loaded_pointers_map) = split_config_map(loaded).unwrap();
    update_config_map_by_pointers(&mut values_map, &loaded_pointers_map).unwrap();
    assert_eq!(values_map["a1"], json!(10));
    assert_eq!(values_map["a1"], values_map["a2"]);
}

#[test]
fn test_replace_pointers() {
    let mut values_map = BTreeMap::from([("a".to_owned(), json!(5)), ("b".to_owned(), json!(7))]);
    let pointers_map =
        BTreeMap::from([("b".to_owned(), "a".to_owned()), ("c".to_owned(), "a".to_owned())]);
    update_config_map_by_pointers(&mut values_map, &pointers_map).unwrap();
    assert_eq!(values_map["a"], values_map["c"]);
    assert_eq!(values_map["b"], json!(7));

    let err = update_config_map_by_pointers(&mut BTreeMap::default(), &pointers_map).unwrap_err();
    assert_matches!(err, ConfigError::PointerTargetNotFound { .. });
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
struct CustomConfig {
    param_path: String,
    #[serde(default)]
    seed: usize,
}

impl SerializeConfig for CustomConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "param_path",
                &self.param_path,
                "This is param_path.",
                ParamPrivacyInput::Public,
            ),
            ser_generated_param(
                "seed",
                SerializationType::Number,
                "A dummy seed with generated default = 0.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

// Loads CustomConfig from args.
fn load_custom_config(args: Vec<&str>) -> CustomConfig {
    let custom_config = CustomConfig { param_path: "default value".to_owned(), seed: 5 };
    load_and_process_util::<CustomConfig>(
        custom_config.dump(),
        args.into_iter().map(|s| s.to_owned()).collect(),
    )
    .unwrap()
}

#[test]
fn test_load_default_config() {
    let param_path = load_custom_config(vec![]).param_path;
    assert_eq!(param_path, "default value");
}

#[test]
fn test_load_custom_config_file() {
    let args = vec!["-f", CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "custom value");
}

#[test]
fn test_load_custom_config_file_and_args() {
    let args = vec![
        "--config_file",
        CUSTOM_CONFIG_PATH.to_str().unwrap(),
        "--param_path",
        "command value",
    ];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "command value");
}

#[test]
fn test_load_many_custom_config_files() {
    let custom_config_path = CUSTOM_CONFIG_PATH.to_str().unwrap();
    let cli_config_param = format!("{custom_config_path},{custom_config_path}");
    let args = vec!["-f", cli_config_param.as_str()];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "custom value");
}

#[test]
fn test_generated_type() {
    assert_eq!(load_custom_config(vec![]).seed, 0);
    assert_eq!(load_custom_config(vec!["--seed", "7"]).seed, 7);
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct RequiredConfig {
    param_path: String,
    num: usize,
}

impl SerializeConfig for RequiredConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_required_param(
                "param_path",
                SerializationType::String,
                "This is param_path.",
                ParamPrivacyInput::Public,
            ),
            ser_param("num", &self.num, "This is num.", ParamPrivacyInput::Public),
        ])
    }
}

// Loads param_path of RequiredConfig from args.
fn load_required_param_path(args: Vec<&str>) -> String {
    let required_config = RequiredConfig { param_path: "default value".to_owned(), num: 3 };
    let loaded_required_config: RequiredConfig = load_and_process_util(
        required_config.dump(),
        args.into_iter().map(|s| s.to_owned()).collect(),
    )
    .unwrap();
    loaded_required_config.param_path
}

#[test]
fn test_negative_required_param() {
    let dumped_config = RequiredConfig { param_path: "0".to_owned(), num: 3 }.dump();
    let err = load_and_process_util::<RequiredConfig>(dumped_config, vec![]).unwrap_err();
    assert_matches!(err, ConfigError::MissingParam { .. });
}

#[test]
fn test_required_param_from_command() {
    let args = vec!["--param_path", "1234"];
    let param_path = load_required_param_path(args);
    assert_eq!(param_path, "1234");
}

#[test]
fn test_required_param_from_file() {
    let args = vec!["--config_file", CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_required_param_path(args);
    assert_eq!(param_path, "custom value");
}
