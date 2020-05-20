use serde::{Deserialize, Serialize};

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum ClientError {
    #[fail(display = "Unknown request Parameter: {}", _0)]
    UnknownParameter(String),

    #[fail(display = "Unknown request Type: {}", _0)]
    UnknownType(String),

    #[fail(display = "Request parameter missing: {}", _0)]
    MissingParameter(String),

    #[fail(display = "Unable to parse provided input data: {}", _0)]
    InvalidInput(String),
}

impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> ClientError {
        ClientError::InvalidInput(format!("{}", e))
    }
}