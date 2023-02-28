/////////////////////////////////////////////////////
///  The main should look something like this:
//          fn main() {
//              let config = ConfigurationBuilder::apply_default()
//                  .apply_env()
//                  .apply_yaml()
//                  .apply_cla()
//                  .build();
//
//              let gateway_config = GatewayConfig::new(config);
//              let storage_config = GatewayConfig::new(config);
//              // More components...
//          }
/////////////////////////////////////////////////////
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead},
};

type ParamPath = String;
type ParamValue = String;

// Stores a mapping from param path to its value.
// Each provider modifies the mapping so after applying all the providers, the mapping contains the
// desired configuration of all the components.
#[derive(Debug)]
pub struct ConfigurationBuilder {
    configuration: HashMap<ParamPath, ParamValue>,
}

#[derive(Debug)]
pub struct BuiltConfiguration {
    configuration: HashMap<ParamPath, ParamValue>,
}

impl ConfigurationBuilder {
    // Reads the default configuration from the default configuration file and adds all the
    // parameters to the configuration mapping.
    pub fn apply_default() -> Self {
        // TODO: use different file format and crates to make this function better.
        let mut configuration = HashMap::new();
        let default_config_file = include_str!("default_configuration_example.txt");
        println!("{default_config_file}");

        for line in default_config_file.lines() {
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let v: Vec<_> = line.split(" = ").collect();
            let k: ParamPath = v[0].to_owned();
            let v: ParamValue = v[1].to_owned();

            configuration.insert(k, v);
        }
        Self { configuration }
    }

    // Reads a configuration file and applies it on the builder mapping.
    pub fn apply_config_file(mut self) -> Self {
        // TODO: implement parsing of config file into a hashmap.
        let file_config = dummy_config();
        for (k, v) in file_config {
            *self.configuration.get_mut(&k).unwrap() = v;
        }
        self
    }

    // Applies env settings on the builder mapping.
    pub fn apply_env(mut self) -> Self {
        // TODO: implement parsing of env variables into a hashmap.
        let env_config = dummy_config();
        for (k, v) in env_config {
            *self.configuration.get_mut(&k).unwrap() = v;
        }
        self
    }

    // Applies command line arguments on the builder mapping.
    pub fn apply_command_line(mut self) -> Self {
        let cla_config = dummy_config();
        for (k, v) in cla_config {
            *self.configuration.get_mut(&k).unwrap() = v;
        }
        self
    }

    pub fn build(self) -> BuiltConfiguration {
        BuiltConfiguration { configuration: self.configuration }
    }
}

// Simulate parsing configuration (Remove after implementing the providers).
fn dummy_config() -> HashMap<String, String> {
    HashMap::new()
}

// TODO: fill other fields of metadata required for configuring a parameter, such as short/long
// naming for command line, type, etc.
type Description = String;
type DefaultValue = String;
type ParamMetadata = (Description, DefaultValue);
// Each components configuration struct should implement this trait (GatewayConfig, StorageConfig,
// etc.)
pub trait Configurable {
    // Reads all the necessary values from the mapping and creates an instance of the components
    // configuration.
    // Should be called after applying all of the providers.
    fn new(built: &BuiltConfiguration) -> Self;

    // Returns the components configuration + metadata.
    // Used for multiple purposes:
    //  1. Creating the default configuration file.
    //  2. Monitoring the node at runtime - getting the configuration in which it runs.
    fn dump(&self) -> Vec<(ParamPath, ParamValue, ParamMetadata)>;
}

/// Here is an example for a dummy component

pub struct ComponentConfig {
    pub param1: String,
    pub param2: String,
}

impl Configurable for ComponentConfig {
    fn new(built: &BuiltConfiguration) -> Self {
        // TODO: implement things like:
        //   - convert from string to type
        //   - replace ${<common param>} with the value of <common param> - for example, chain_id is
        //     a common param for multiple component so it should be configured only in one place.
        //   - call sub component constructors
        //   - make it shorter
        Self {
            param1: built
                .configuration
                .get("component.param1")
                .expect("component.param1 not in the configuration.")
                .clone(),
            param2: built
                .configuration
                .get("component.param2")
                .expect("component.param2 not in the configuration.")
                .clone(),
        }
    }

    fn dump(&self) -> Vec<(ParamPath, ParamValue, ParamMetadata)> {
        vec![
            (
                "component.param1".to_owned(),
                self.param1.clone(),
                ("Description of param1".to_owned(), "default_param1".to_owned()),
            ),
            (
                "component.param2".to_owned(),
                self.param2.clone(),
                ("Description of param2".to_owned(), "default_param2".to_owned()),
            ),
        ]
    }
}

#[test]
fn simulate_main() {
    let config = ConfigurationBuilder::apply_default()
        .apply_env()
        .apply_config_file()
        .apply_command_line()
        .build();

    let component_config = ComponentConfig::new(&config);
    let config_dump = component_config.dump();
    println!("{config_dump:#?}");
    let expected_value = config_dump[0].2.1.clone(); // The default value from the param metadata
    assert_eq!(component_config.param1, expected_value);
}
