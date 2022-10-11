use std::net::SocketAddr;

use reqwest::Client;
use starknet_api::core::ChainId;

use super::GatewayConfig;

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

pub fn get_test_chain_id() -> ChainId {
    ChainId("SN_GOERLI".to_string())
}

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: get_test_chain_id(),
        server_ip: String::from("127.0.0.1:0"),
        max_events_chunk_size: 10,
        max_events_keys: 10,
    }
}
