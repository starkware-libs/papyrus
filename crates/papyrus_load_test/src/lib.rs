pub mod transactions;
pub mod scenarios;

use goose::goose::{GooseUser, TransactionError};
use serde_json::json;
type PostResult = Result<serde_json::Value, Box<TransactionError>>;

pub async fn post_jsonrpc_request(user: &mut GooseUser, request: &serde_json::Value) -> PostResult {
    let response = user.post_json("", request).await?.response.map_err(|e| Box::new(e.into()))?;
    let response = response.json::<serde_json::Value>().await.map_err(|e| Box::new(e.into()))?;
    Ok(response)
}

pub fn jsonrpc_request(method: &str, params: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}