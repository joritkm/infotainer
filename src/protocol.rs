use std::convert::TryFrom;

use actix::prelude::{Addr, Message};
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

/// Represents a server-sent message
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Response {
    pub data: String,
}

/// Represents an accepted Submission that can be stored and distributed
#[derive(Debug, PartialEq, Eq, Hash, Clone, Deserialize, Serialize)]
pub struct Publication {
    pub data: String,
}

impl From<&ClientSubmission> for Publication {
    fn from(submission: &ClientSubmission) -> Self {
        Publication {
            data: submission.data.clone(),
        }
    }
}

/// Represents data intended for distribution to subscribers of Subscription `id`
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ClientSubmission {
    pub id: Uuid,
    pub data: String,
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
    /// Publish a new message to subscribed clients
    Publish { param: ClientSubmission },
}

/// Represents a message from a connected client,
/// including the clients identifying uuid and a request
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ClientMessage {
    pub id: Uuid,
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
        let id_string = format!("{}", id.to_hyphenated());
        let client_id_uuid = Uuid::from(id);
        let client_id_string: String = format!("{}", client_id_uuid);
        assert_eq!(id_string, client_id_string);
    }

    #[test]
    fn test_get_message() {
        let client_id = Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap();
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let get: ClientMessage = serde_json::from_str(
            r#"{
                "id": "52b43d1e-9945-482c-900a-86125589e937",
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
        let client_id = Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap();
        let list = ClientMessage::try_from(
            r#"{
                "id": "52b43d1e-9945-482c-900a-86125589e937",
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
        let client_id = Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap();
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let add = ClientMessage::try_from(
            r#"{
                "id": "52b43d1e-9945-482c-900a-86125589e937",
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
        let client_id = Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap();
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let remove = ClientMessage::try_from(
            r#"{
                "id": "52b43d1e-9945-482c-900a-86125589e937",
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
        let client_id = Uuid::from_str("52b43d1e-9945-482c-900a-86125589e937").unwrap();
        let dummy_subscription_id = Uuid::from_str("9dd27e53-0918-4adc-bbec-08cd27a3ab7f").unwrap();
        let submission = ClientSubmission {
            id: dummy_subscription_id,
            data: "Test publication".to_owned(),
        };
        let publish: ClientMessage = serde_json::from_str(
            r#"{
                "id": "52b43d1e-9945-482c-900a-86125589e937",
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
