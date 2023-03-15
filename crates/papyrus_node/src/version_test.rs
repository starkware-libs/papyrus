use super::{Metadata, Version};

#[test]
fn version() {
    const MAJOR: u32 = 1;
    const MINOR: u32 = 2;
    const PATCH: u32 = 3;
    let expected_version = format!("{}.{}.{}", MAJOR, MINOR, PATCH);

    // Stable.
    let expected_version_stable = format!("{}-{}", expected_version, Metadata::Stable);
    let version_stable =
        Version { major: MAJOR, minor: MINOR, patch: PATCH, meta: Metadata::Stable };
    assert_eq!(version_stable.to_string(), expected_version);
    assert_eq!(version_stable.version_with_metadata(), expected_version_stable);

    // Dev.
    let expected_version_dev = format!("{}-{}", expected_version, Metadata::Dev);
    let version_dev = Version { major: MAJOR, minor: MINOR, patch: PATCH, meta: Metadata::Dev };
    assert_eq!(version_dev.to_string(), expected_version);
    assert_eq!(version_dev.version_with_metadata(), expected_version_dev);
}
