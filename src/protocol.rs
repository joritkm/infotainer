use std::convert::TryFrom;
use std::fmt;

use actix::prelude::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;

/// Represents a requested task from a connected client
#[derive(Debug, PartialEq, Clone)]
pub enum ClientRequest {
    /// Get information on a specific subscription or retrieve the
    /// current subscription index and its metadata.
    Get { param: String },
    /// Add client to a Subscription, creating it, if it doesn't exist
    Add { param: String },
    /// Remove client from a Subscription, deleting it, if the Subscription
    /// was created by client
    Remove { param: String },
    /// Publish a new message to subscribed clients
    Publish { param: String },
}

impl TryFrom<&str> for ClientRequest {
    type Error = ClientError;

    /// Attempt to create a `ClientRequest` from a raw string, received on the websocket, by
    /// implementing the following protocol:
    /// * A valid client request consists of the request type and corresponding parameters.
    /// * The string received on the socket must separate request type from request parameters by '::'.
    /// * The request type must be on the left side of the separator.
    /// * The request type must be one of "get", "add", "remove", "publish". This is case insensitive.
    ///TODO: Include rules for request parameters
    fn try_from(raw: &str) -> Result<ClientRequest, Self::Error> {
        let mut params = raw.split("::");
        let req_type = params
            .next()
            .ok_or(ClientError::MissingParameter(String::from("Request type")))?
            .to_lowercase();
        let req_arg: String = params
            .next()
            .ok_or(ClientError::MissingParameter(String::from(
                "Request Argument",
            )))?
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(256)
            .collect();
        match req_type.as_str() {
            "get" => Ok(ClientRequest::Get { param: req_arg }),
            "add" => Ok(ClientRequest::Add { param: req_arg }),
            "remove" => Ok(ClientRequest::Remove { param: req_arg }),
            "publish" => Ok(ClientRequest::Publish { param: req_arg }),
            _ => Err(ClientError::UnknownType(String::from(raw))),
        }
    }
}

/// Representing a client identified by its uuid
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ClientID {
    uid: Uuid,
}

impl ClientID {
    /// Return identifying uuid
    pub fn id(&self) -> Uuid {
        self.uid
    }
}

impl From<Uuid> for ClientID {
    /// Create a new ClientID from a uuid
    fn from(id: Uuid) -> ClientID {
        ClientID { uid: id }
    }
}

impl TryFrom<&str> for ClientID {
    type Error = uuid::Error;

    /// Attempt to create a new ClientID from simple string representation of an indentifying uuid
    fn try_from(id: &str) -> Result<ClientID, Self::Error> {
        let uid = Uuid::parse_str(&id)?;
        Ok(ClientID { uid: uid })
    }
}

impl fmt::Display for ClientID {
    /// Format ClientID to simple string representation of identifying uuid
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.uid.to_simple())
    }
}

/// Represents a message from a connected client,
/// including the clients identifying uuid and a request
#[derive(Debug, PartialEq, Clone, Message)]
#[rtype(result = "()")]
pub struct ClientMessage {
    pub id: ClientID,
    pub req: ClientRequest,
}

impl TryFrom<&str> for ClientMessage {
    /// Attempts to create a ClientMessage from a string received on
    /// the websocket. Checks if the message follows this protocol:
    /// * The message consists of two parts: an identifying uuid and a request
    /// * Identifying uuid and request are separated by "|"
    ///TODO: move id matching to own method and sanitize properly
    ///TODO: check with ClientSessions if id exists and map permissions
    type Error = ClientError;
    fn try_from(raw: &str) -> Result<ClientMessage, Self::Error> {
        let mut msg_token = raw.split("|");
        let msg_id = msg_token
            .next()
            .ok_or(ClientError::MissingIdentification(String::from(
                "No valid ID found in message",
            )))?;
        let msg_req = msg_token
            .next()
            .ok_or(ClientError::MissingRequest(String::from(
                "No valid request found in message",
            )))?;
        let id = ClientID::try_from(msg_id)
            .map_err(|e| ClientError::MissingIdentification(String::from("Could not parse ID")))?;
        let request = ClientRequest::try_from(msg_req)?;
        Ok(ClientMessage {
            id: id,
            req: request,
        })
    }
}
