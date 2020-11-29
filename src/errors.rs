use serde::{Deserialize, Serialize};
use uuid;

/// Wraps internal errors caused by client interaction
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::str::FromStr;
    use uuid::Uuid;

    #[test]
    fn test_client_error () {
        let err = ClientError::InvalidInput(String::from("Test"));
        let err_display = format!("{}", err);
        assert_eq!("Invalid Input: Test", &err_display);
    }

    #[test]
    fn test_wrapping_cbor_errors () {
        if let Err(e) = serde_cbor::from_slice::<String>(&[23]) {
            let err = ClientError::from(e);
            assert_eq!("Invalid Input: invalid type: integer `23`, expected a string", format!("{}",err))
        }
    }
    
    #[test]
    fn test_wrapping_uuid_errors () {
        if let Err(e) = Uuid::from_str("notauuidstring") {
            let err = ClientError::from(e);
            assert_eq!("Invalid Input: invalid length: expected one of [36, 32], found 14", format!("{}",err))
        }
    }
}