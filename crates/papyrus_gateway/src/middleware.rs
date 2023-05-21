use hyper::{Body, Request};
use jsonrpsee::core::http_helpers::read_body;
use tower::BoxError;

use crate::api::version_config::{get_latest_version_id, VersionState, VERSION_CONFIG};
use crate::SERVER_MAX_BODY_SIZE;

pub async fn proxy_request(req: Request<Body>) -> Result<Request<Body>, BoxError> {
    let uri = &req.uri().clone();
    let prefix = get_version_as_prefix(uri.path())?;
    let (parts, body) = req.into_parts();
    let (body_bytes, is_single) = match read_body(&parts.headers, body, SERVER_MAX_BODY_SIZE).await
    {
        Ok(res) => res,
        Err(err) => return Err(BoxError::from(err)),
    };
    let new_body = match is_single {
        true => {
            let body = serde_json::from_slice::<jsonrpsee::types::Request<'_>>(&body_bytes)?;
            add_version_to_method_name_in_body(vec![body], prefix)
        }
        false => {
            let vec_body =
                serde_json::from_slice::<Vec<jsonrpsee::types::Request<'_>>>(&body_bytes)?;
            add_version_to_method_name_in_body(vec_body, prefix)
        }
    }?;
    Ok(Request::from_parts(parts, new_body.into()))
}

fn add_version_to_method_name_in_body(
    mut vec_body: Vec<jsonrpsee::types::Request>,
    prefix: &str,
) -> Result<Vec<u8>, BoxError> {
    let vec_body = vec_body
        .iter_mut()
        .map(|body| {
            let Some(stripped_method) = strip_starknet_from_method(body.method.as_ref()) else {
                return Err(BoxError::from("Method name has unexpected format"))
            };
            body.method = format!("starknet_{}_{}", prefix, stripped_method).into();
            Ok(body)
        })
        .collect::<Vec<_>>();
    let vec_body = match vec_body.iter().all(|body| body.is_ok()) {
        true => vec_body.iter().map(|body| body.as_ref().unwrap()).collect::<Vec<_>>(),
        false => return Err(BoxError::from("Method name has unexpected format")),
    };
    serde_json::to_vec(&vec_body).or_else(|err| Err(BoxError::from(err)))
}

/// this assumes that all methods are of the form:
/// starknet_OnlyOneUnderScoreAndMethodNameIsCamleCased
fn strip_starknet_from_method(method: &str) -> Option<&str> {
    let split_method_name = method.split('_').collect::<Vec<_>>();
    split_method_name.get(1).and_then(|res| Some(res.clone()))
}

fn get_version_as_prefix(path: &str) -> Result<&str, BoxError> {
    let latest_version_id = get_latest_version_id();
    let prefix = match path {
        "/" | "" => latest_version_id,
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
