use actix::prelude::SendError;
use serde::{Deserialize, Serialize};
use uuid;

use crate::data_log::DataLogRequest;

/// Represents errors caused by client interaction
#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum ClientError {
    #[fail(display = "Invalid Input: {}", _0)]
    InvalidInput(String),
}

impl From<serde_cbor::Error> for ClientError {
    fn from(e: serde_cbor::Error) -> ClientError {
        ClientError::InvalidInput(format!("{}", e))
    }
}

impl From<uuid::Error> for ClientError {
    fn from(e: uuid::Error) -> ClientError {
        ClientError::InvalidInput(format!("{}", e))
    }
}

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum DataLogError {
    #[fail(display = "Encountered error during file system interaction: {}", _0)]
    FileSystem(String),

    #[fail(display = "Sending DataLog message to DataLogger failed: {}", _0)]
    DataLogRequest(String),

    #[fail(display = "Could not serialize data for writing to disk: {}", _0)]
    SerializerError(String),
}

impl From<SendError<DataLogRequest>> for DataLogError {
    fn from(e: SendError<DataLogRequest>) -> DataLogError {
        DataLogError::DataLogRequest(format!("{}", e))
    }
}

impl From<std::io::Error> for DataLogError {
    fn from(e: std::io::Error) -> DataLogError {
        DataLogError::FileSystem(format!("{}", e))
    }
}

impl From<serde_cbor::Error> for DataLogError {
    fn from(e: serde_cbor::Error) -> DataLogError {
        DataLogError::SerializerError(format!("{}", e))
    }
}

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum PublicationError {
    #[fail(display = "Could not log publication to the data log: {}", _0)]
    DataLoggingError(String),
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::str::FromStr;
    use uuid::Uuid;

    #[test]
    fn test_client_error() {
        let err = ClientError::InvalidInput(String::from("Test"));
        let err_display = format!("{}", err);
        assert_eq!("Invalid Input: Test", &err_display);
    }

    #[test]
    fn test_wrapping_cbor_errors() {
        if let Err(e) = serde_cbor::from_slice::<String>(&[23]) {
            let err = ClientError::from(e);
            assert_eq!(
                "Invalid Input: invalid type: integer `23`, expected a string",
                format!("{}", err)
            )
        }
    }

    #[test]
    fn test_wrapping_uuid_errors() {
        if let Err(e) = Uuid::from_str("notauuidstring") {
            let err = ClientError::from(e);
            assert_eq!(
                "Invalid Input: invalid length: expected one of [36, 32], found 14",
                format!("{}", err)
            )
        }
    }
}
