#[derive(Eq, PartialEq, Hash)]
/// Labels the jsonRPC versions we have such that there can be multiple versions that are supported,
/// and there can be multiple versions that are deprecated.
/// Supported -> method exposed via the http path "/version_id" (e.g. http://host:port/V0_3_0)
/// Deprecated -> method not exposed.
#[derive(Clone, Copy, Debug)]
pub enum VersionState {
    // TODO: nevo - remove the dead_code attribute once other versions are implemented - hides
    // "Supported" and "Deprecated" not constructed error
    Supported,
    #[allow(dead_code)]
    Deprecated,
}

/// latest version must be set as supported
pub const VERSION_CONFIG: &[(&str, VersionState)] =
    &[(VERSION_0_3_0, VersionState::Supported), (VERSION_0_4_0, VersionState::Supported)];
pub const VERSION_0_3_0: &str = "V0_3_0";
pub const VERSION_0_4_0: &str = "V0_4_0";
