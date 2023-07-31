/// Errors that may be returned from a reader client.
#[derive(thiserror::Error, Debug)]
pub enum ReaderClientError {
    // The variants of ClientError are duplicated here so that the user won't need to know about
    // ClientError.
    /// A client error representing bad status http responses.
    #[error("Bad response status code: {:?} message: {:?}.", code, message)]
    BadResponseStatus { code: StatusCode, message: String },
    /// A client error representing http request errors.
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    /// A client error representing errors that might be solved by retrying mechanism.
    #[error("Retry error code: {:?}, message: {:?}.", code, message)]
    RetryError { code: RetryErrorCode, message: String },
    /// A client error representing deserialization errors.
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    /// A client error representing errors returned by the starknet client.
    #[error(transparent)]
    StarknetError(#[from] StarknetError),
    /// A client error representing errors from [`starknet_api`].
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    /// A client error representing transaction receipts errors.
    #[error(transparent)]
    TransactionReceiptsError(#[from] TransactionReceiptsError),
    #[error("Invalid transaction: {:?}, error: {:?}.", tx_hash, msg)]
    BadTransaction { tx_hash: TransactionHash, msg: String },
}

impl From<ClientError> for ReaderClientError {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::BadResponseStatus { code, message } => {
                ReaderClientError::BadResponseStatus { code, message }
            }
            ClientError::RequestError(err) => ReaderClientError::RequestError(err),
            ClientError::RetryError { code, message } => {
                ReaderClientError::RetryError { code, message }
            }
            ClientError::SerdeError(err) => ReaderClientError::SerdeError(err),
            ClientError::StarknetError(err) => ReaderClientError::StarknetError(err),
        }
    }
}
