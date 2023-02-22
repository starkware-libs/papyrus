pub mod create_request;
pub mod scenarios;
pub mod transactions;
use goose::goose::{GooseUser, TransactionError};
use serde::Deserialize;
use serde_json::{json, Value as jsonVal};
type PostResult = Result<jsonVal, Box<TransactionError>>;

pub async fn post_jsonrpc_request(user: &mut GooseUser, request: &jsonVal) -> PostResult {
    let response = user.post_json("", request).await?.response.map_err(|e| Box::new(e.into()))?;
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

pub fn get_last_block_number() -> u64 {
    let file_content = include_str!("resources/last_block_number.txt");
    file_content.parse::<u64>().unwrap_or_else(|_| {
        panic!("\"{}\" is not a legal format for last_block_number", file_content)
    })
}
