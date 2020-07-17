use std::convert::TryFrom;
use std::fmt;

use actix::prelude::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;
use crate::pubsub::Publication;

/// Represents a requested task from a connected client
#[derive(Debug, PartialEq, Clone)]
pub enum ClientRequest {
    /// List all currently available subscriptions
    List,
    /// Get information on a specific subscription
    Get { param: Uuid },
    /// Add client to a Subscription, creating it, if it doesn't exist
    Add { param: Uuid },
    /// Remove client from a Subscription, deleting it, if the Subscription
    /// was created by client
    Remove { param: Uuid },
    /// Publish a new message to subscribed clients
    Publish { param: (Uuid, Publication) },
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
            .take(256)
            .collect();
        match req_type.as_str() {
            "list" => Ok(ClientRequest::List),
            "get" => Ok(ClientRequest::Get {
                param: Uuid::parse_str(&req_arg)?,
            }),
            "add" => Ok(ClientRequest::Add {
                param: Uuid::parse_str(&req_arg)?,
            }),
            "remove" => Ok(ClientRequest::Remove {
                param: Uuid::parse_str(&req_arg)?,
            }),
            "publish" => Ok({
                let sub_id = Uuid::parse_str(&req_arg[..36])?;
                let data: Publication = serde_json::from_str(&req_arg[36..])?;
                ClientRequest::Publish {
                    param: (sub_id, data),
                }
            }),
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
#[rtype(result = "Result<(),ClientError>")]
pub struct ClientMessage {
    pub id: ClientID,
    pub req: ClientRequest,
}

impl TryFrom<&str> for ClientMessage {
    /// Attempts to create a ClientMessage from a string received on
    /// the websocket. Checks if the message follows this protocol:
    /// * The message consists of two parts: an identifying uuid and a request
    /// * Identifying uuid and request are separated by "|"
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
            .map_err(|_e| ClientError::MissingIdentification(String::from("Could not parse ID")))?;
        let request = ClientRequest::try_from(msg_req)?;
        Ok(ClientMessage {
            id: id,
            req: request,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_client_id() {
        let id = Uuid::new_v4();
        let id_string = format!("{}", id.to_simple());
        let client_id_uuid = ClientID::from(id);
        let client_id_string: String = format!("{}", client_id_uuid);
        assert_eq!(id_string, client_id_string);
    }

    #[test]
    fn test_message_protocol() {
        let client_id = ClientID::from(Uuid::new_v4());
        let publication = Publication::new(&"Test publication".to_owned());
        let publication_string = serde_json::to_string(&publication).unwrap();
        let dummy_subscription_id = Uuid::new_v4();
        let get = ClientMessage::try_from(format!("{}|get::{}", client_id, dummy_subscription_id).as_str()).unwrap();
        let list = ClientMessage::try_from(format!("{}|list::", client_id).as_str()).unwrap();
        let add =
            ClientMessage::try_from(format!("{}|add::{}", client_id, dummy_subscription_id).as_str())
                .unwrap();
        let remove = ClientMessage::try_from(
            format!("{}|remove::{}", client_id, dummy_subscription_id).as_str(),
        )
        .unwrap();
        let publish = ClientMessage::try_from(
            format!(
                "{}|publish::{}{}",
                client_id, dummy_subscription_id, publication_string
            )
            .as_str(),
        )
        .unwrap();
        assert_eq!(
            get,
            ClientMessage {
                id: client_id.clone(),
                req: ClientRequest::Get {
                    param: dummy_subscription_id
                }
            }
        );
        assert_eq!(
            add,
            ClientMessage {
                id: client_id.clone(),
                req: ClientRequest::Add {
                    param: dummy_subscription_id
                }
            }
        );
        assert_eq!(
            remove,
            ClientMessage {
                id: client_id.clone(),
                req: ClientRequest::Remove {
                    param: dummy_subscription_id
                }
            }
        );
        assert_eq!(
            list,
            ClientMessage {
                id: client_id.clone(),
                req: ClientRequest::List
            }
        );
        assert_eq!(
            publish,
            ClientMessage {
                id: client_id.clone(),
                req: ClientRequest::Publish {
                    param: (dummy_subscription_id, publication)
                }
            }
        );
    }
}
