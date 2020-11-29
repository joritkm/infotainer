use std::collections::HashSet;
use std::convert::{TryFrom, TryInto};
use std::hash::Hash;

use actix::prelude::{Addr, Message};
use actix_web::web::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;
use crate::websocket::WebSocketSession;

/// Represents a message sent by the server to a connected client
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ServerMessage<T>(T)
where
    T: Serialize;

impl From<&Publication> for ServerMessage<Publication> {
    fn from(publication: &Publication) -> ServerMessage<Publication> {
        ServerMessage(publication.clone())
    }
}

impl From<&Response> for ServerMessage<Response> {
    fn from(resp: &Response) -> ServerMessage<Response> {
        ServerMessage(resp.clone())
    }
}

/// Represents a request to add a new websocket session to the pubsub server
#[derive(Debug, PartialEq, Clone, Message)]
#[rtype("()")]
pub struct ClientJoin {
    pub id: Uuid,
    pub addr: Addr<WebSocketSession>,
}

/// Represents a request to remove a websocket session from the pubsub server
#[derive(Debug, PartialEq, Clone, Message)]
#[rtype("()")]
pub struct ClientDisconnect {
    pub id: Uuid,
}

/// Represents an accepted Submission that can be stored and distributed
#[derive(Debug, PartialEq, Eq, Hash, Clone, Deserialize, Serialize)]
pub struct Publication {
    pub data: Vec<u8>,
}

impl From<&ClientSubmission> for Publication {
    fn from(submission: &ClientSubmission) -> Self {
        Publication {
            data: submission.data.to_owned(),
        }
    }
}

/// Represents data sent by the server in response to a ClientRequest
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum Response {
    List { data: Vec<Uuid> },
    Add { data: Uuid },
    Get { data: HashSet<Publication> },
    Remove { data: Uuid },
    Empty
}

/// Represents data intended for distribution to subscribers of Subscription `id`
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ClientSubmission {
    pub id: Uuid,
    pub data: Vec<u8>,
}

/// Represents a command from a connected client
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum ClientRequest {
    /// List all currently available subscriptions
    List,
    /// Get a Subscription's log
    Get { param: Uuid },
    /// Add client to a Subscription, creating it, if it doesn't exist
    Add { param: Uuid },
    /// Remove client from a Subscription, deleting it, if the Subscription
    /// was created by client
    Remove { param: Uuid },
    /// Submit new data for publication to subscribed clients
    Submit { param: ClientSubmission },
}

/// Represents a message from a connected client,
/// including the clients identifying uuid and a request
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ClientMessage {
    pub id: Uuid,
    pub request: ClientRequest,
}

impl TryFrom<&Bytes> for ClientMessage {
    /// Attempts to create a ClientMessage from cbor encoded binary data received on
    /// the websocket.

    type Error = serde_cbor::Error;
    fn try_from(raw: &Bytes) -> Result<ClientMessage, serde_cbor::Error> {
        serde_cbor::from_slice::<ClientMessage>(&raw[..])
    }
}

impl TryInto<Bytes> for ClientMessage {
    // Attemtps to create actix_web::web::Bytes from cbor encoded ClientMessages

    type Error = serde_cbor::Error;
    fn try_into(self) -> Result<Bytes, serde_cbor::Error> {
        Ok(Bytes::from(serde_cbor::to_vec(&self)?))
    }
}
