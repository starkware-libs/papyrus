use hyper::{Body, Request};
use jsonrpsee::core::http_helpers::read_body;
use tower::BoxError;

use crate::version_config::version_config::{VersionState, LATEST_VERSION_ID, VERSION_CONFIG};
use crate::SERVER_MAX_BODY_SIZE;

/// [`Tower`] middleware intended to proxy method requests to the right version of the API.
/// The middleware reads the JsonRPC request body and request path
/// then prefixes the method name with the appropriate version identifier.
/// It returns a new [`hyper::Request`] object with the new method name.
///
/// # Arguments
/// * req - [`hyper::Request`] object passed by the server.
///
/// # Examples
/// ```ignore
/// use papyrus_gateway::SERVER_MAX_BODY_SIZE;
/// use papyrus_gateway::middleware::papyrus_gateway;
/// use jsonrpsee::server::ServerBuilder;
///
/// #[tokio::main]
/// async fn main() {
///     let server = ServerBuilder::default(
///         .max_request_body_size(SERVER_MAX_BODY_SIZE)
///         .set_middleware(tower::ServiceBuilder::new().filter_async(proxy_request))
///         .build(&config.server_address)
///         .await?;
/// };
/// ```
///
/// [`Tower`]: https://crates.io/crates/tower
pub(crate) async fn proxy_request(req: Request<Body>) -> Result<Request<Body>, BoxError> {
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

fn add_version_to_method_name_in_body(
    mut vec_body: Vec<jsonrpsee::types::Request<'_>>,
    _prefix: &str,
    is_single: bool,
) -> Result<Vec<u8>, BoxError> {
    let Ok(vec_body) = vec_body
        .iter_mut()
        .map(|body| {
            let Some(stripped_method) = strip_starknet_from_method(body.method.as_ref()) else {
                return Err(BoxError::from("Method name has unexpected format"))
            };
            body.method = format!("starknet_{}", stripped_method).into();
            Ok(body)
        })
        .collect::<Result<Vec<_>, _>>() else { return Err(BoxError::from("Method name has unexpected format")) };
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

fn get_version_as_prefix(path: &str) -> Result<&str, BoxError> {
    let prefix = match path {
        "/" | "" => LATEST_VERSION_ID,
        path => {
            // get the version name from the path (should be something like "http://host:port/version_id")
            let Some(version) = path.split('/').collect::<Vec<_>>().pop() else {
                return Err(BoxError::from("Invalid path format"));
            };
            let Some((version_id, _)) =
                // find a matching version in the version config
                VERSION_CONFIG.iter().find(|(verison_id, version_state)| {
                    *verison_id == version && *version_state != VersionState::Deprecated
                }) else {
                return Err(BoxError::from("Invalid path, couldn't find matching version"));
            };
            *version_id
        }
    };
    Ok(prefix)
}
