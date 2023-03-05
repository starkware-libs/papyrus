pub mod create_request;
pub mod scenarios;
pub mod transactions;
use goose::goose::{GooseUser, TransactionError};
use serde::Deserialize;
use serde_json::{json, Value as jsonVal};
type PostResult = Result<jsonVal, Box<TransactionError>>;

pub async fn post_jsonrpc_request(user: &mut GooseUser, request: &jsonVal) -> PostResult {
    let response = user.post_json("", request).await?.response.map_err(|e| Box::new(e.into()))?;
    // The purpose of this struct and the line afterward is to report on failed requests.
    // The "response.json::<TransactionReceiptResponse>" deserialize the body of response to
    // TransactionReceiptResponse. If the response is an error, the result field doesn't exist in
    // the body, the deserialization will fail, and the function will return an error.
    #[derive(Deserialize)]
    struct TransactionReceiptResponse {
        result: jsonVal,
    }
    let response =
        response.json::<TransactionReceiptResponse>().await.map_err(|e| Box::new(e.into()))?;
    Ok(response.result)
}

pub fn jsonrpc_request(method: &str, params: jsonVal) -> jsonVal {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}
