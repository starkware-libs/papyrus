use std::net::SocketAddr;

use reqwest::Client;

// TODO(anatg): See if this can be usefull for the benchmark testing as well.
pub async fn send_request(
    address: SocketAddr,
    method: &str,
    params: &str,
) -> Result<serde_json::Value, anyhow::Error> {
    let client = Client::new();
    let res_str = client
        .post(format!("http://{:?}", address))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{"jsonrpc":"2.0","id":"1","method":"{}","params":[{}]}}"#,
            method, params
        ))
        .send()
        .await?
        .text()
        .await?;
    Ok(serde_json::from_str(&res_str)?)
}
