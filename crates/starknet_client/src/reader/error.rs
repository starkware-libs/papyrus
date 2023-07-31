#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;

use std::fmt::{self, Display, Formatter};

use serde::de::Error;
use serde::{Deserialize, Serialize};
use starknet_api::transaction::TransactionHash;
use starknet_api::StarknetApiError;

pub use crate::reader::objects::block::TransactionReceiptsError;
use crate::starknet_error::{KnownStarknetErrorCode, StarknetErrorCode};
use crate::{ClientError, RetryErrorCode, StarknetError, StatusCode};

/// A Starknet error code that can be returned when reading data from Starknet.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ReaderStarknetErrorCode {
    UnknownErrorCode(String),
    #[serde(rename = "StarknetErrorCode.BLOCK_NOT_FOUND")]
    BlockNotFound,
    #[serde(rename = "StarkErrorCode.MALFORMED_REQUEST")]
    MalformedRequest,
    #[serde(rename = "StarknetErrorCode.OUT_OF_RANGE_CLASS_HASH")]
    OutOfRangeClassHash,
    #[serde(rename = "StarknetErrorCode.UNDECLARED_CLASS")]
    UndeclaredClass,
}

#[derive(thiserror::Error, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReaderStarknetError {
    pub code: ReaderStarknetErrorCode,
    pub message: String,
}

impl Display for ReaderStarknetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

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
    StarknetError(#[from] ReaderStarknetError),
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
            ClientError::StarknetError(err) => match err.try_into() {
                Ok(reader_starknet_error) => {
                    ReaderClientError::StarknetError(reader_starknet_error)
                }
                Err(serde_error) => ReaderClientError::SerdeError(serde_error),
            },
        }
    }
}

impl TryFrom<StarknetError> for ReaderStarknetError {
    type Error = serde_json::Error;

    fn try_from(error: StarknetError) -> Result<Self, Self::Error> {
        let StarknetError { code, message } = error;
        let reader_code = match code {
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound) => {
                ReaderStarknetErrorCode::BlockNotFound
            }
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::MalformedRequest) => {
                ReaderStarknetErrorCode::MalformedRequest
            }
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::OutOfRangeClassHash) => {
                ReaderStarknetErrorCode::OutOfRangeClassHash
            }
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::UndeclaredClass) => {
                ReaderStarknetErrorCode::UndeclaredClass
            }
            StarknetErrorCode::UnknownErrorCode(unknown_code) => {
                ReaderStarknetErrorCode::UnknownErrorCode(unknown_code)
            }
            other_code => {
                let json_value = serde_json::to_value(&other_code)?;
                let other_code_str =
                    json_value.as_str().ok_or(serde_json::Error::custom(format!(
                        "Starknet error code {:?} wasn't serialized into a JSON string",
                        other_code
                    )))?;
                ReaderStarknetErrorCode::UnknownErrorCode(other_code_str.to_string())
            }
        };
        Ok(ReaderStarknetError { code: reader_code, message })
    }
}
