use hyper::{Body, Request};
use jsonrpsee::core::http_helpers::read_body;
use tower::BoxError;

use crate::api::version_config::{get_latest_version_id, VersionState, VERSION_CONFIG};
use crate::SERVER_MAX_BODY_SIZE;

pub async fn proxy_request(req: Request<Body>) -> Result<Request<Body>, BoxError> {
    let prefix = get_prefix(req.uri().path())?;
    let (parts, body) = req.into_parts();
    let (body_bytes, is_single) =
        read_body(&parts.headers, body, SERVER_MAX_BODY_SIZE).await.unwrap();
    let new_body = if is_single {
        let mut body =
            serde_json::from_slice::<jsonrpsee::types::Request<'_>>(&body_bytes).unwrap();
        body.method =
            format!("starknet_{}_{}", prefix, strip_starknet_from_method(body.method.to_string()))
                .into();
        serde_json::to_vec(&body)
    } else {
        let mut vec_body =
            serde_json::from_slice::<Vec<jsonrpsee::types::Request<'_>>>(&body_bytes).unwrap();
        let vec_body = vec_body
            .iter_mut()
            .map(|body| {
                body.method = format!(
                    "starknet_{}_{}",
                    prefix,
                    strip_starknet_from_method(body.method.to_string())
                )
                .into();
                body
            })
            .collect::<Vec<_>>();
        serde_json::to_vec(&vec_body)
    };
    Ok(Request::from_parts(parts, new_body.unwrap().into()))
}

/// this assumes that all methods are of the form:
/// starknet_OnlyOneUnderScoreAndMethodNameIsCamleCased
fn strip_starknet_from_method(method: String) -> String {
    let split_method_name = method.split('_').collect::<Vec<_>>();
    String::from(*split_method_name.get(1).unwrap())
}

fn get_prefix(path: &str) -> Result<String, BoxError> {
    let prefix: String;
    if let Some(latest_version_id) = get_latest_version_id() {
        prefix = match path {
            "/" | "" => latest_version_id,
            path => {
                if let Some(version) = path.to_string().split('/').collect::<Vec<_>>().pop() {
                    if let Some((version_id, _)) =
                        VERSION_CONFIG.iter().find(|(verison_id, version_state)| {
                            *verison_id == version && *version_state != VersionState::Deprecated
                        })
                    {
                        (*version_id).to_string()
                    } else {
                        return Err(BoxError::from("Invalid path, couldn't find matching version"));
                    }
                } else {
                    latest_version_id
                }
            }
        };
    } else {
        return Err(BoxError::from("latest version not found"));
    }
    Ok(prefix)
}
