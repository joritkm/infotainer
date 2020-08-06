use std::convert::TryFrom;
use std::fmt;

use actix::prelude::{Addr, Message};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;
use crate::websocket::WebSocketSession;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ServerMessageData {
    Publication { id: Uuid, data: String },
    Response { data: String },
}

impl From<&ClientSubmission> for ServerMessageData {
    fn from(submission: &ClientSubmission) -> ServerMessageData {
        ServerMessageData::Publication {
            id: submission.id.clone(),
            data: submission.data.to_owned(),
        }
    }
}

impl From<&String> for ServerMessageData {
    fn from(message: &String) -> ServerMessageData {
        ServerMessageData::Response {
            data: message.to_owned(),
        }
    }
}

/// Represents a message sent by the server to a connected client
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ServerMessage<T>
where
    T: Serialize,
{
    pub content: T,
}

#[derive(Debug, PartialEq, Clone, Message)]
#[rtype("()")]
pub struct ClientJoin {
    pub id: ClientID,
    pub addr: Addr<WebSocketSession>,
}

#[derive(Debug, PartialEq, Clone, Message)]
#[rtype("()")]
pub struct ClientDisconnect {
    pub id: ClientID,
}

///Represents client data intended for publication
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct ClientSubmission {
    pub id: Uuid,
    pub data: String,
}

/// Represents a command from a connected client
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
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
    Publish { param: ClientSubmission },
}

/// Representing a client identified by its uuid
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
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
/// ## Schema
/// ```json
/// {
///   "id": "Uuid",
///   "req": {
///     "ClientRequest": {
///       "param": (Uuid|ClientSubmission)
///     }
///   }
/// }
/// ```
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ClientMessage {
    pub id: ClientID,
    pub request: ClientRequest,
}

impl TryFrom<&str> for ClientMessage {
    /// Attempts to create a ClientMessage from a json-string received on
    /// the websocket.

    type Error = serde_json::Error;
    fn try_from(raw: &str) -> Result<ClientMessage, Self::Error> {
        serde_json::from_str::<ClientMessage>(raw)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_client_id() {
        let id = Uuid::new_v4();
        let id_string = format!("{}", id.to_simple());
        let client_id_uuid = ClientID::from(id);
        let client_id_string: String = format!("{}", client_id_uuid);
        assert_eq!(id_string, client_id_string);
    }

    #[test]
    fn test_get_message() {
        let client_id =
            ClientID::from(Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap());
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let get: ClientMessage = serde_json::from_str(
            r#"{"id": { 
                    "uid": "52b43d1e-9945-482c-900a-86125589e937"
                },
                "request": { 
                    "Get": {
                        "param": "9dd27e53-0918-4adc-bbec-08cd27a3ab7f"
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            get,
            ClientMessage {
                id: client_id.clone(),
                request: ClientRequest::Get {
                    param: dummy_subscription_id
                }
            }
        );
    }

    #[test]
    fn test_list_message() {
        let client_id =
            ClientID::from(Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap());
        let list = ClientMessage::try_from(
            r#"{
                "id": {
                    "uid": "52b43d1e-9945-482c-900a-86125589e937"
                },
                "request": "List"
            }"#,
        )
        .unwrap();
        assert_eq!(
            list,
            ClientMessage {
                id: client_id.clone(),
                request: ClientRequest::List
            }
        );
    }

    #[test]
    fn test_add_message() {
        let client_id =
            ClientID::from(Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap());
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let add = ClientMessage::try_from(
            r#"{
                "id": { 
                    "uid": "52b43d1e-9945-482c-900a-86125589e937"
                },
                "request": { 
                    "Add": {
                        "param": "9dd27e53-0918-4adc-bbec-08cd27a3ab7f"
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            add,
            ClientMessage {
                id: client_id.clone(),
                request: ClientRequest::Add {
                    param: dummy_subscription_id
                }
            }
        );
    }

    #[test]
    fn test_remove_message() {
        let client_id =
            ClientID::from(Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap());
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let remove = ClientMessage::try_from(
            r#"{
                "id": {
                    "uid": "52b43d1e-9945-482c-900a-86125589e937"
                },
                "request": { 
                    "Remove": {
                        "param": "9dd27e53-0918-4adc-bbec-08cd27a3ab7f"
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            remove,
            ClientMessage {
                id: client_id.clone(),
                request: ClientRequest::Remove {
                    param: dummy_subscription_id
                }
            }
        );
    }

    #[test]
    fn test_publish_message() {
        let client_id =
            ClientID::from(Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap());
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let submission = ClientSubmission {
            id: dummy_subscription_id,
            data: "Test publication".to_owned(),
        };
        let publish: ClientMessage = serde_json::from_str(
            r#"{
                "id": {
                    "uid": "52b43d1e-9945-482c-900a-86125589e937"
                },
                "request": { 
                    "Publish": {
                        "param": {
                            "id": "9dd27e53-0918-4adc-bbec-08cd27a3ab7f",
                            "data": "Test publication"
                        }
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            publish,
            ClientMessage {
                id: client_id.clone(),
                request: ClientRequest::Publish { param: submission }
            }
        );
    }
}
