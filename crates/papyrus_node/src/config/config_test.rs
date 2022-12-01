use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use yaml_rust::yaml::Hash;
use yaml_rust::{Yaml, YamlLoader};

use crate::config::{parse_yaml, Config, ConfigError};

#[test]
fn parse_valid_yaml() {
    // Simulate initial config.
    let k_1 = Yaml::String("k_1".to_owned());
    let k_2 = Yaml::String("k_2".to_owned());
    let k_3 = Yaml::String("k_3".to_owned());
    let v_1 = Yaml::String("v1_initial".to_owned());
    let v_2 = Yaml::Integer(0);
    let v_3 = Yaml::String("v3_initial".to_owned());
    let mut config = Hash::new();
    config.insert(k_1.clone(), v_1);
    config.insert(k_2.clone(), v_2);
    config.insert(k_3.clone(), v_3);

    // Simulate a valid yaml input (all the params in the input are valid, some params are not set
    // in the yaml).
    let yaml = YamlLoader::load_from_str(
        r#"
sec_1:
    k_1: from_yaml # String
    k_2: 2   # Integer
    "#,
    )
    .unwrap();
    let input = &yaml[0];
    let sec_1_config = input["sec_1"].as_hash().unwrap();

    parse_yaml("sec_1", &mut config, sec_1_config).unwrap();

    // The params k_1, k_2 are set from the input, k_3 remains as it was initialized.
    assert_eq!(
        config.get(&k_1).expect("k_1 not in config").as_str().expect("v_1 is not a string"),
        "from_yaml"
    );
    assert_eq!(
        config.get(&k_2).expect("k_2 not in config").as_i64().expect("v_2 is not an integer"),
        2
    );
    assert_eq!(
        config.get(&k_3).expect("k_3 not in config").as_str().expect("v_3 is not a string"),
        "v3_initial"
    );
}

#[test]
fn parse_invalid_yaml() {
    // Simulate initial config.
    let k_1 = Yaml::String("k_1".to_owned());
    let v_1 = Yaml::String("v1_initial".to_owned());
    let mut config = Hash::new();
    config.insert(k_1, v_1);

    // Simulate invalid yaml input (k_2 not in the configuration).
    let yaml = YamlLoader::load_from_str(
        r#"
sec_1:
    k_1: from_yaml
    k_2: 2
    "#,
    )
    .unwrap();
    let input = &yaml[0];
    let sec_1_config = input["sec_1"].as_hash().unwrap();
    let res = parse_yaml("sec_1", &mut config, sec_1_config);

    assert_matches!(res, Err(ConfigError::YamlKey {
        section,
        key,
    }) if section == "sec_1" && key == Yaml::String("k_2".to_owned()));
}

#[test]
fn load_default_config() {
    let workspace_root = Path::new("../../");
    env::set_current_dir(workspace_root).expect("Couldn't set working dir.");
    // TODO(spapini): Move the config closer.
    Config::load().expect("Failed to load the config.");
}
