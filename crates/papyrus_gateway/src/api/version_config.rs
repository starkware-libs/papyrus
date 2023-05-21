#[derive(Eq, PartialEq, Hash)]
/// Labels the jsonRPC versions we have such that one and only one version is Latest,
/// there can be multiple versions that are supported (latest is implicitly supported), and there
/// can be multiple versions that are deprecated.
/// Latest -> method exposed via the http path "/" and "" (e.g. http://host:port/)
/// Supported -> method exposed via the http path "/version_id" (e.g. http://host:port/V0_3_0)
/// Deprecated -> method not exposed.
#[derive(Clone, Copy)]
pub enum VersionState {
    // TODO: nevo - remove the dead_code attribute once other versions are implemented - hides
    // "Supported" and "Deprecated" not constructed error
    Latest,
    #[allow(dead_code)]
    Supported,
    #[allow(dead_code)]
    Deprecated,
}

pub const VERSION_0_3_0: &str = "V0_3_0";
pub const VERSION_CONFIG: &[(&str, VersionState)] = &[(VERSION_0_3_0, VersionState::Latest)];

pub const fn get_latest_version_id() -> &'static str {
    let mut i = 0;
    let n = VERSION_CONFIG.len();
    while i < n {
        let (version_id, version_state) = VERSION_CONFIG[i];
        match version_state {
            VersionState::Latest => return version_id,
            _ => i += 1,
        }
    }
    // this would never be returned, it's just for compilations purposes (if n == 0).
    return "";
}
