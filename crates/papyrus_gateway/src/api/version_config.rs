#[derive(Eq, PartialEq, Hash)]
#[allow(dead_code)] // TODO: nevo - remove this once other versions are implemented - hides "Supported" and "Deprecated" not constructed error
pub enum VersionState {
    Latest,
    Supported,
    Deprecated,
}

pub const VERSION_0_3_0: &str = "V0_3_0";
pub const VERSION_CONFIG: &[(&str, VersionState)] = &[(VERSION_0_3_0, VersionState::Latest)];

pub fn get_latest_version_id() -> Option<String> {
    VERSION_CONFIG
        .iter()
        .find(|(_, version_state)| version_state == &VersionState::Latest)
        .map(|res| res.0.to_string())
}
