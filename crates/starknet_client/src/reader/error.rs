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
