use super::load_config;

#[test]
fn load_config_test() {
    // TODO(spapini): Move the config closer.
    let _config = load_config("../../../config/config.ron").expect("Failed to load the config.");
}
