use std::convert::TryFrom;
use std::fmt;

use serde::{Deserialize,Serialize};
use uuid::Uuid;
use actix_web::error;

use crate::errors::ClientError;

#[derive(Debug, PartialEq, Clone)]
enum ClientRequest {
    Get { param: String },
    Add { param: String },
    Remove { param: String },
}

impl ClientRequest {
    fn from_str(raw: &str) -> Result<ClientRequest, error::Error> {
        let mut params = raw.split("::");
        let req_type = params
            .next()
            .ok_or(ClientError::MissingParameter(String::from("Request type")))
            .map_err(|e| error::ErrorBadRequest(e))?
            .to_lowercase();
        let req_arg: String = params
            .next()
            .ok_or(ClientError::MissingParameter(String::from(
                "Request Argument",
            )))
            .map_err(|e| error::ErrorBadRequest(e))?
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(256)
            .collect();
        match req_type.as_str() {
            "get" => Ok(ClientRequest::Get { param: req_arg }),
            "add" => Ok(ClientRequest::Add { param: req_arg }),
            "remove" => Ok(ClientRequest::Remove { param: req_arg }),
            _ => Err(ClientError::UnknownType(String::from(raw)))
                .map_err(|e| error::ErrorBadRequest(e)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ClientID {
    uid: Uuid,
}

impl ClientID {
    pub fn id(&self) -> Uuid {
        self.uid
    }
}

impl From<Uuid> for ClientID {
    fn from(id: Uuid) -> ClientID {
        ClientID { uid: id }
    }
}

impl TryFrom<&str> for ClientID {
    type Error = uuid::Error;

    fn try_from(id: &str) -> Result<ClientID, Self::Error> {
        let uid = Uuid::parse_str(&id)?;
        Ok(ClientID { uid: uid })
    }
}

impl fmt::Display for ClientID {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.uid.to_simple())
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ClientMessage {
    id: ClientID,
    req: ClientRequest,
}

impl ClientMessage {
    pub fn new(raw: &str) -> Result<ClientMessage, error::Error> {
        let mut msg_token = raw.split("|");
        let msg_id = msg_token
            .next()
            .ok_or(error::ErrorForbidden("Missing identification."))?;
        let msg_req = msg_token
            .next()
            .ok_or(error::ErrorBadRequest("Missing Request."))?;
        let id = ClientID::try_from(msg_id).map_err(|e| error::ErrorForbidden(e))?;
        let request = ClientRequest::from_str(msg_req)?;
        Ok(ClientMessage {
            id: id,
            req: request,
        })
    }
}