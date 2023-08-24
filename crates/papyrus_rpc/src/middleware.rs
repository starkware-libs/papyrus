use hyper::{Body, Request};
use jsonrpsee::core::http_helpers::read_body;
use regex::Regex;
use tower::BoxError;
use tracing::{debug, instrument};

use crate::version_config::{VersionState, VERSION_CONFIG, VERSION_PATTERN};
use crate::SERVER_MAX_BODY_SIZE;

/// [`Tower`] middleware intended to proxy method requests to the right version of the API.
/// The middleware reads the JsonRPC request body and request path
/// then prefixes the method name with the appropriate version identifier.
/// It returns a new [`hyper::Request`] object with the new method name.
///
/// # Arguments
/// * req - [`hyper::Request`] object passed by the server.
///
/// [`Tower`]: https://crates.io/crates/tower
pub(crate) async fn proxy_rpc_request(req: Request<Body>) -> Result<Request<Body>, BoxError> {
    debug!("proxy_rpc_request -> Request received: {:?}", req);
    let uri = &req.uri().clone();
    let prefix = get_version_as_prefix(uri.path())?;
    let (parts, body) = req.into_parts();
    let (body_bytes, is_single) =
        read_body(&parts.headers, body, SERVER_MAX_BODY_SIZE).await.map_err(BoxError::from)?;
    let new_body = match is_single {
        true => {
            let body = serde_json::from_slice::<jsonrpsee::types::Request<'_>>(&body_bytes)?;
            add_version_to_method_name_in_body(vec![body], prefix, is_single)
        }
        false => {
            let vec_body =
                serde_json::from_slice::<Vec<jsonrpsee::types::Request<'_>>>(&body_bytes)?;
            add_version_to_method_name_in_body(vec_body, prefix, is_single)
        }
    }?;
    Ok(Request::from_parts(parts, new_body.into()))
}

/// ['Tower`] middleware intended to deny requests with unsupported paths.
/// supported paths are paths that starts with '/rpc/' followed by a supported version id.
///
/// # Arguments
/// * req - [`hyper::Request`] object passed by the server.
///
/// [`Tower`]: https://crates.io/crates/tower
pub(crate) async fn deny_requests_with_unsupported_path(
    req: Request<Body>,
) -> Result<Request<Body>, BoxError> {
    debug!("deny_requests_with_unsupported_path -> Request received: {:?}", req);
    let uri = req.uri();
    match is_supported_path(uri.path()) {
        true => Ok(req),
        false => Err(BoxError::from("Unsupported path for request")),
    }
}

fn add_version_to_method_name_in_body(
    mut vec_body: Vec<jsonrpsee::types::Request<'_>>,
    prefix: &str,
    is_single: bool,
) -> Result<Vec<u8>, BoxError> {
    let Ok(vec_body) = vec_body
        .iter_mut()
        .map(|body| {
            let Some(stripped_method) = strip_starknet_from_method(body.method.as_ref()) else {
                return Err(BoxError::from("Method name has unexpected format"));
            };
            body.method = format!("starknet_{prefix}_{stripped_method}").into();
            Ok(body)
        })
        .collect::<Result<Vec<_>, _>>()
    else {
        return Err(BoxError::from("Method name has unexpected format"));
    };
    let serialized = match is_single {
        true => serde_json::to_vec(&vec_body[0]),
        false => serde_json::to_vec(&vec_body),
    };
    serialized.map_err(BoxError::from)
}

/// this assumes that all methods are of the form:
/// starknet_OnlyOneUnderScoreAndMethodNameIsCamleCased
fn strip_starknet_from_method(method: &str) -> Option<&str> {
    let split_method_name = method.split('_').collect::<Vec<_>>();
    split_method_name.get(1).copied()
}

#[instrument(level = "debug", err)]
fn get_version_as_prefix(path: &str) -> Result<&str, BoxError> {
    // get the version name from the path (should be something like "http://host:port/rpc/version_id")
    let uri_components = &mut path.split('/').collect::<Vec<_>>();
    let Some(temp_version) = uri_components.get(2) else {
        // as long as 'deny_requests_with_unsupported_path' middleware is used, this should never
        // happen but for safety we return an error and not unreachable!()
        let msg = format!("Invalid path format: {path}");
        debug!(msg);
        return Err(BoxError::from(msg));
    };
    let version_comps = temp_version.split('_').collect::<Vec<_>>();
    let mut patch_num = 0;
    if version_comps.len() == 3 {
        // if the version is of the form "V0_0_1" we store the path num to check that our latest
        // supported patch version is at least the requested version.
        patch_num = version_comps[2].parse::<u8>().map_err(BoxError::from)?;
    }
    let version = format!("{}_{}", version_comps[0], version_comps[1]);
    let Some((version_id, _)) =
        // find a matching version in the version config
        VERSION_CONFIG.iter().find(|(version_id, version_state)| {
            (version_id.name == version || version_id.name.to_lowercase() == version) && *version_state != VersionState::Deprecated && patch_num <= version_id.patch
        }) else {
        return Err(BoxError::from(format!("Invalid path, couldn't find matching version for version_id: {version}")));
    };
    Ok(version_id.name)
}

fn is_supported_path(path: &str) -> bool {
    let re = Regex::new((r"^\/rpc\/".to_string() + VERSION_PATTERN + "$").as_str())
        .expect("should be a valid regex");
    re.is_match(path)
}
