#[tokio::test]
async fn retry_error_codes() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    for (status_code, error_code) in [
        (StatusCode::TEMPORARY_REDIRECT, RetryErrorCode::Redirect),
        (StatusCode::REQUEST_TIMEOUT, RetryErrorCode::Timeout),
        (StatusCode::TOO_MANY_REQUESTS, RetryErrorCode::TooManyRequests),
        (StatusCode::SERVICE_UNAVAILABLE, RetryErrorCode::ServiceUnavailable),
        (StatusCode::GATEWAY_TIMEOUT, RetryErrorCode::Timeout),
    ] {
        let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=latest")
            .with_status(status_code.as_u16().into())
            .expect(5)
            .create();
        let error = starknet_client.block_number().await.unwrap_err();
        assert_matches!(error, ReaderClientError::ClientError(ClientError::RetryError { code, message: _ }) if code == error_code);
        mock.assert();
    }
}
