#[cfg(test)]
#[path = "version_config_test.rs"]
mod version_config_test;

use std::fmt;

pub const VERSION_PATTERN: &str = "[Vv][0-9]+_[0-9]+(_[0-9]+)?";

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

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub struct VersionId {
    // TODO(yair): change to enum so that the match in get_methods_from_supported_apis can be
    // exhaustive.
    pub name: &'static str,
    pub patch: u8,
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.name, self.patch)
    }
}

/// latest version must be set as supported
pub const VERSION_CONFIG: &[(VersionId, VersionState)] = &[
    (VERSION_0_4, VersionState::Supported),
    (VERSION_0_5, VersionState::Supported),
    (VERSION_0_6, VersionState::Supported),
    (VERSION_0_7, VersionState::Supported),
];
pub const VERSION_0_4: VersionId = VersionId { name: "V0_4", patch: 0 };
pub const VERSION_0_5: VersionId = VersionId { name: "V0_5", patch: 1 };
pub const VERSION_0_6: VersionId = VersionId { name: "V0_6", patch: 0 };
pub const VERSION_0_7: VersionId = VersionId { name: "V0_7", patch: 0 };
