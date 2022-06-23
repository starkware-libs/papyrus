use crate::config::load_config;

#[test]
fn load_config_test() {
    let _config = load_config("config/config.ron").expect("Failed to load the config.");
}
