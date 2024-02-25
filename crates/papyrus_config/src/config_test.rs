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
use validator::Validate;

use crate::command::{get_command_matches, update_config_map_by_command_args};
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
    load,
    load_and_process_config,
    split_pointers_map,
    split_values_and_types,
    update_config_map_by_pointers,
    update_optional_values,
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
        let (mut dumped, _) = split_values_and_types(outer_config.dump());
        update_optional_values(&mut dumped);
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
    let command = Command::new("Testing");
    let dumped_config =
        TypicalConfig { a: Duration::from_secs(1), b: "bbb".to_owned(), c: false }.dump();
    let args = vec!["Testing", "--a", "1234", "--b", "15"];
    env::set_var("C", "true");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();

    let arg_matches = get_command_matches(&dumped_config, command, args).unwrap();
    let (mut config_map, required_map) = split_values_and_types(dumped_config);
    update_config_map_by_command_args(&mut config_map, &required_map, &arg_matches).unwrap();

    assert_eq!(json!(1234), config_map["a"]);
    assert_eq!(json!("15"), config_map["b"]);
    assert_eq!(json!(true), config_map["c"]);

    let loaded_config: TypicalConfig = load(&config_map).unwrap();
    assert_eq!(Duration::from_millis(1234), loaded_config.a);
}

#[test]
fn test_env_nested_params() {
    let command = Command::new("Testing");
    let dumped_config = OuterConfig {
        opt_elem: Some(1),
        opt_config: Some(InnerConfig { o: 2 }),
        inner_config: InnerConfig { o: 3 },
    }
    .dump();
    let args = vec!["Testing", "--opt_elem", "1234"];
    env::set_var("OPT_CONFIG____IS_NONE__", "true");
    env::set_var("INNER_CONFIG__O", "4");
    let args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();

    let arg_matches = get_command_matches(&dumped_config, command, args).unwrap();
    let (mut config_map, required_map) = split_values_and_types(dumped_config);
    update_config_map_by_command_args(&mut config_map, &required_map, &arg_matches).unwrap();

    assert_eq!(json!(1234), config_map["opt_elem"]);
    assert_eq!(json!(true), config_map["opt_config.#is_none"]);
    assert_eq!(json!(4), config_map["inner_config.o"]);

    update_optional_values(&mut config_map);

    let loaded_config: OuterConfig = load(&config_map).unwrap();
    assert_eq!(Some(1234), loaded_config.opt_elem);
    assert_eq!(None, loaded_config.opt_config);
    assert_eq!(4, loaded_config.inner_config.o);
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
fn test_nested_config_presentation() {
    let configs = vec![
        OuterConfig {
            opt_elem: Some(1),
            opt_config: Some(InnerConfig { o: 2 }),
            inner_config: InnerConfig { o: 3 },
        },
        OuterConfig {
            opt_elem: None,
            opt_config: Some(InnerConfig { o: 2 }),
            inner_config: InnerConfig { o: 3 },
        },
        OuterConfig { opt_elem: Some(1), opt_config: None, inner_config: InnerConfig { o: 3 } },
    ];

    for config in configs {
        let presentation = get_config_presentation(&config, true).unwrap();
        let keys: Vec<_> = presentation.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["inner_config", "opt_config", "opt_elem"]);
        let public_presentation = get_config_presentation(&config, false).unwrap();
        let keys: Vec<_> = public_presentation.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["inner_config", "opt_config", "opt_elem"]);
    }
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
    let (loaded_config_map, loaded_pointers_map) = split_pointers_map(loaded);
    let (mut config_map, _) = split_values_and_types(loaded_config_map);
    update_config_map_by_pointers(&mut config_map, &loaded_pointers_map).unwrap();
    assert_eq!(config_map["a1"], json!(10));
    assert_eq!(config_map["a1"], config_map["a2"]);
}

#[test]
fn test_replace_pointers() {
    let (mut config_map, _) = split_values_and_types(BTreeMap::from([ser_param(
        "a",
        &json!(5),
        "This is a.",
        ParamPrivacyInput::Public,
    )]));
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
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config.json");
    CustomConfig { param_path: "default value".to_owned(), seed: 5 }
        .dump_to_file(&vec![], file_path.to_str().unwrap())
        .unwrap();

    load_and_process_config::<CustomConfig>(
        File::open(file_path).unwrap(),
        Command::new("Program"),
        args.into_iter().map(|s| s.to_owned()).collect(),
    )
    .unwrap()
}

#[test]
fn test_load_default_config() {
    let args = vec!["Testing"];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "default value");
}

#[test]
fn test_load_custom_config_file() {
    let args = vec!["Testing", "-f", CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_custom_config(args).param_path;
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
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "command value");
}

#[test]
fn test_load_many_custom_config_files() {
    let custom_config_path = CUSTOM_CONFIG_PATH.to_str().unwrap();
    let cli_config_param = format!("{custom_config_path},{custom_config_path}");
    let args = vec!["Testing", "-f", cli_config_param.as_str()];
    let param_path = load_custom_config(args).param_path;
    assert_eq!(param_path, "custom value");
}

#[test]
fn test_generated_type() {
    let args = vec!["Testing"];
    assert_eq!(load_custom_config(args).seed, 0);

    let args = vec!["Testing", "--seed", "7"];
    assert_eq!(load_custom_config(args).seed, 7);
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
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config.json");
    RequiredConfig { param_path: "default value".to_owned(), num: 3 }
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
fn test_negative_required_param() {
    let dumped_config = RequiredConfig { param_path: "0".to_owned(), num: 3 }.dump();
    let (config_map, _) = split_values_and_types(dumped_config);
    let err = load::<RequiredConfig>(&config_map).unwrap_err();
    assert_matches!(err, ConfigError::MissingParam { .. });
}

#[test]
fn test_required_param_from_command() {
    let args = vec!["Testing", "--param_path", "1234"];
    let param_path = load_required_param_path(args);
    assert_eq!(param_path, "1234");
}

#[test]
fn test_required_param_from_file() {
    let args = vec!["Testing", "--config_file", CUSTOM_CONFIG_PATH.to_str().unwrap()];
    let param_path = load_required_param_path(args);
    assert_eq!(param_path, "custom value");
}

#[test]
fn deeply_nested_optionals() {
    #[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
    struct Level0 {
        level0_value: u8,
        level1: Option<Level1>,
    }

    impl SerializeConfig for Level0 {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            let mut res = BTreeMap::from([ser_param(
                "level0_value",
                &self.level0_value,
                "This is level0_value.",
                ParamPrivacyInput::Public,
            )]);
            res.extend(ser_optional_sub_config(&self.level1, "level1"));
            res
        }
    }

    #[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
    struct Level1 {
        pub level1_value: u8,
        pub level2: Option<Level2>,
    }

    impl SerializeConfig for Level1 {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            let mut res = BTreeMap::from([ser_param(
                "level1_value",
                &self.level1_value,
                "This is level1_value.",
                ParamPrivacyInput::Public,
            )]);
            res.extend(ser_optional_sub_config(&self.level2, "level2"));
            res
        }
    }

    #[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
    struct Level2 {
        pub level2_value: Option<u8>,
    }

    impl SerializeConfig for Level2 {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            ser_optional_param(
                &self.level2_value,
                1,
                "level2_value",
                "This is level2_value.",
                ParamPrivacyInput::Public,
            )
        }
    }

    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("config2.json");
    Level0 { level0_value: 1, level1: None }
        .dump_to_file(&vec![], file_path.to_str().unwrap())
        .unwrap();

    let l0 = load_and_process_config::<Level0>(
        File::open(file_path.clone()).unwrap(),
        Command::new("Testing"),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(l0, Level0 { level0_value: 1, level1: None });

    let l1 = load_and_process_config::<Level0>(
        File::open(file_path.clone()).unwrap(),
        Command::new("Testing"),
        vec!["Testing".to_owned(), "--level1.#is_none".to_owned(), "false".to_owned()],
    )
    .unwrap();
    assert_eq!(
        l1,
        Level0 { level0_value: 1, level1: Some(Level1 { level1_value: 0, level2: None }) }
    );

    let l2 = load_and_process_config::<Level0>(
        File::open(file_path.clone()).unwrap(),
        Command::new("Testing"),
        vec![
            "Testing".to_owned(),
            "--level1.#is_none".to_owned(),
            "false".to_owned(),
            "--level1.level2.#is_none".to_owned(),
            "false".to_owned(),
        ],
    )
    .unwrap();
    assert_eq!(
        l2,
        Level0 {
            level0_value: 1,
            level1: Some(Level1 { level1_value: 0, level2: Some(Level2 { level2_value: None }) }),
        }
    );

    let l2_value = load_and_process_config::<Level0>(
        File::open(file_path).unwrap(),
        Command::new("Testing"),
        vec![
            "Testing".to_owned(),
            "--level1.#is_none".to_owned(),
            "false".to_owned(),
            "--level1.level2.#is_none".to_owned(),
            "false".to_owned(),
            "--level1.level2.level2_value.#is_none".to_owned(),
            "false".to_owned(),
        ],
    )
    .unwrap();
    assert_eq!(
        l2_value,
        Level0 {
            level0_value: 1,
            level1: Some(Level1 {
                level1_value: 0,
                level2: Some(Level2 { level2_value: Some(1) }),
            }),
        }
    );
}
