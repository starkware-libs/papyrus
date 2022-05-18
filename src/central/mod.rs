mod api;

use api::CentralApi;
use url::Url;

#[derive(Debug)]
pub struct ClientError {}

pub struct CentralClient {
    central_url: Url,
    internal_client: reqwest::Client,
}

#[allow(dead_code)]
impl CentralClient {
    fn new(central_url_str: &str) -> CentralClient {
        CentralClient {
            central_url: Url::parse(central_url_str).unwrap(),
            internal_client: reqwest::Client::new(),
        }
    }

    async fn request(&self, path: &str) -> Result<String, Box<dyn std::error::Error>> {
        let joined = self.central_url.join(path)?;
        let res = self.internal_client.get(joined).send().await?;
        let body = res.text().await?;
        Ok(body)
    }
}

#[async_trait::async_trait]
impl CentralApi for CentralClient {
    async fn block_number(&self) -> Result<u32, ClientError> {
        let block_bumber = self
            .request("feeder_gateway/get_last_batch_id")
            .await
            .unwrap();
        Ok(block_bumber.parse().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;

    #[tokio::test]
    async fn test_get_block_number() {
        let central_client = CentralClient::new(&mockito::server_url());
        let mock = mock("GET", "/feeder_gateway/get_last_batch_id")
            .with_status(200)
            .with_header("content-type", "text/plain; charset=utf-8")
            .with_header("content-length", "6")
            .with_header("date", "Wed, 18 May 2022 20:35:52 GMT")
            .with_header("server", "Python/3.7 aiohttp/3.8.1")
            .with_header("via", "1.1 google")
            .with_header(
                "alt-svc",
                "h3=\":443\"; ma=2592000,h3-29=\":443\"; ma=2592000",
            )
            .with_body("195812")
            .create();
        let block_number = central_client.block_number().await.unwrap();
        mock.assert();
        assert_eq!(block_number, 195812);
    }

    #[tokio::test]
    async fn test_get_block_number_alpha() {
        let central_client = CentralClient::new("https://alpha4.starknet.io");
        let _block_number = central_client.block_number().await.unwrap();
    }
}
