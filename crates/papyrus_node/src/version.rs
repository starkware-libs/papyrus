#[cfg(test)]
#[path = "version_test.rs"]
mod version_test;

use std::fmt::Display;

/// Major version component of the current release.
const VERSION_MAJOR: u32 = 0;

/// Minor version component of the current release.
const VERSION_MINOR: u32 = 0;

/// Patch version component of the current release.
const VERSION_PATCH: u32 = 1;

/// Version metadata to append to the version string.
/// Expected values are `dev` and `stable`.
const VERSION_META: Metadata = Metadata::Dev;
const DEV_VERSION_META: &str = "dev";
const STABLE_VERSION_META: &str = "stable";

#[allow(dead_code)]
#[derive(PartialEq)]
enum Metadata {
    Dev,
    Stable,
}

impl Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Metadata::Dev => f.write_str(DEV_VERSION_META),
            Metadata::Stable => f.write_str(STABLE_VERSION_META),
        }
    }
}

#[derive(PartialEq)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
    meta: Metadata,
}

impl Default for Version {
    fn default() -> Self {
        Self {
            major: VERSION_MAJOR,
            minor: VERSION_MINOR,
            patch: VERSION_PATCH,
            meta: VERSION_META,
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.version())
    }
}
impl Version {
    /// Returns the textual version string.
    pub fn version(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
    /// Returns the textual version string including the metadata and .
    pub fn version_with_metadata(&self) -> String {
        format!("{}-{}", self.version(), self.meta)
    }
}
