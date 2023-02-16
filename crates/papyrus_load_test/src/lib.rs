pub mod create_request;
pub mod scenarios;
pub mod transactions;

use goose::goose::{GooseUser, TransactionError};
use serde_json::{json, Value as jsonVal};
type PostResult = Result<jsonVal, Box<TransactionError>>;

pub async fn post_jsonrpc_request(user: &mut GooseUser, request: &jsonVal) -> PostResult {
    let response = user.post_json("", request).await?.response.map_err(|e| Box::new(e.into()))?;
    let response = response.json::<jsonVal>().await.map_err(|e| Box::new(e.into()))?;
    Ok(response)
}

pub fn jsonrpc_request(method: &str, params: jsonVal) -> jsonVal {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}
