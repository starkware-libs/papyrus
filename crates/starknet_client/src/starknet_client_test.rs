use assert_matches::assert_matches;
use mockito::mock;
use reqwest::StatusCode;

use crate::test_utils::retry::{get_test_config, MAX_RETRIES};
use crate::{ClientError, RetryErrorCode, StarknetClient};

const NODE_VERSION: &str = "NODE VERSION";
const URL_SUFFIX: &str = "/query";

#[tokio::test]
async fn request_with_retry_max_retries_reached() {
    let starknet_client = StarknetClient::new(None, NODE_VERSION, get_test_config()).unwrap();
    for (status_code, error_code) in [
        (StatusCode::TEMPORARY_REDIRECT, RetryErrorCode::Redirect),
        (StatusCode::REQUEST_TIMEOUT, RetryErrorCode::Timeout),
        (StatusCode::TOO_MANY_REQUESTS, RetryErrorCode::TooManyRequests),
        (StatusCode::SERVICE_UNAVAILABLE, RetryErrorCode::ServiceUnavailable),
        (StatusCode::GATEWAY_TIMEOUT, RetryErrorCode::Timeout),
    ] {
        let mock = mock("GET", URL_SUFFIX)
            .with_status(status_code.as_u16().into())
            .expect(MAX_RETRIES + 1)
            .create();
        let mut url = mockito::server_url().clone();
        url.push_str(URL_SUFFIX);
        let result =
            starknet_client.request_with_retry(starknet_client.internal_client.get(&url)).await;
        assert_matches!(
            result, Err(ClientError::RetryError { code, message: _ }) if code == error_code
        );
        mock.assert();
    }
}
