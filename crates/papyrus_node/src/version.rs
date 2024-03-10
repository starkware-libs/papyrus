#[cfg(test)]
#[path = "version_test.rs"]
mod version_test;

/// Major version component of the current release.
const VERSION_MAJOR: u32 = 0;

/// Minor version component of the current release.
const VERSION_MINOR: u32 = 3;

/// Patch version component of the current release.
const VERSION_PATCH: u32 = 0;

/// Version metadata to append to the version string.
/// Expected values are `dev` and `stable`.
#[allow(dead_code)]
const VERSION_META: Metadata = Metadata::Stable;

/// Textual version string.
pub const VERSION: &str = version_str();
/// Textual version string including the metadata.
pub const VERSION_FULL: &str = full_version_str();

#[allow(dead_code)]
const DEV_VERSION_META: &str = "dev";
#[allow(dead_code)]
const STABLE_VERSION_META: &str = "stable";

#[allow(dead_code)]
#[derive(PartialEq)]
enum Metadata {
    Dev,
    Stable,
}

#[cfg_attr(coverage_nightly, coverage_attribute)]
const fn version_str() -> &'static str {
    const_format::concatcp!(VERSION_MAJOR, ".", VERSION_MINOR, ".", VERSION_PATCH)
}

#[cfg_attr(coverage_nightly, coverage_attribute)]
const fn full_version_str() -> &'static str {
    match VERSION_META {
        Metadata::Dev => const_format::concatcp!(VERSION, "-", DEV_VERSION_META),
        Metadata::Stable => VERSION,
    }
}

#[allow(dead_code)]
const fn metadata_str(metadata: Metadata) -> &'static str {
    match metadata {
        Metadata::Dev => DEV_VERSION_META,
        Metadata::Stable => STABLE_VERSION_META,
    }
}
