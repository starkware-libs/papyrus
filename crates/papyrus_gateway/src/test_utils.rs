use std::env;
use std::fs::read_to_string;
use std::net::SocketAddr;
use std::path::Path;

use reqwest::Client;

// TODO(anatg): Remove this and use the same function when it will be in the storage test_utils.
pub fn read_resource_file(path_in_resource_dir: &str) -> Result<String, anyhow::Error> {
    let path = Path::new(&env::current_dir().expect("Problem with the current directory."))
        .join("resources")
        .join(path_in_resource_dir);
    Ok(read_to_string(path.to_str().unwrap())?.replace('\n', "").replace(' ', ""))
}

// TODO(anatg): See if this can be usefull for the benchmark testing as well.
pub async fn send_request(
    address: SocketAddr,
    method: &str,
    params: &str,
) -> Result<String, anyhow::Error> {
    let client = Client::new();
    Ok(client
        .post(format!("http://{:?}", address))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{"jsonrpc":"2.0","id":"1","method":"{}","params":[{}]}}"#,
            method, params
        ))
        .send()
        .await?
        .text()
        .await?)
}
