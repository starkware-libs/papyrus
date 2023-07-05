use pretty_assertions::assert_eq;

#[test]
fn version() {
    let expected_version =
        format!("{}.{}.{}", super::VERSION_MAJOR, super::VERSION_MINOR, super::VERSION_PATCH);
    assert_eq!(super::VERSION, expected_version);

    let expected_version_with_meta = match super::VERSION_META {
        crate::version::Metadata::Dev => {
            format!("{}-{}", expected_version, super::metadata_str(super::VERSION_META))
        }
        crate::version::Metadata::Stable => expected_version,
    };
    assert_eq!(super::VERSION_FULL, expected_version_with_meta);
}
