use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use actix::prelude::{Actor, Context, Handler, Message, Recipient};

use crate::errors::ClientError;
use crate::protocol::{ClientMessage, ClientRequest};
use crate::subscription::{Subscription, Subscriptions};

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize, Message)]
#[rtype(result = "()")]
///Represents messages received and sent through `PubSubServer`
pub struct Publication {
    id: Uuid,
    data: String,
}

impl Publication {
    ///Creates a new `Publication` with a unique ID
    pub fn new(data: &String) -> Publication {
        Publication {
            id: Uuid::new_v4(),
            data: String::from(data),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
pub struct PubSubServer {
    /// The subscription store
    subs: Subscriptions,
    /// Sessions are represented by the uid of a `ClientID` and
    /// a clients `WebSocketSession` address
    sessions: HashMap<Uuid, Recipient<Publication>>,
}

impl PubSubServer {
    /// Creates a new `PubSubServer`
    pub fn new() -> Result<PubSubServer, ClientError> {
        let subs = Subscriptions::new();
        Ok(PubSubServer {
            subs: subs,
            sessions: HashMap::new(),
        })
    }

    /// Sends a `Publication` to all subscribers of a `Subscription`
    fn publish(&self, sub_id: &Uuid, p: &Publication) -> Result<Vec<String>, ClientError> {
        match self.subs.fetch(sub_id) {
            Ok(sub) => {
                let res = sub.subscribers.iter().map(|s| {
                    match self.sessions.get(&s.id()) {
                        Some(recipient) => {
                            recipient.do_send(p.clone()).unwrap();
                            format!("{}", &s.id().to_hyphenated())
                        },
                        _ => {
                            format!("No session found for ClientID: {}", &s.id())
                        }
                    }
                }).collect();
                Ok(res)
            }
            Err(e) => Err(ClientError::InvalidInput(format!("{}", e))),
        }
    }
}

impl Actor for PubSubServer {
    type Context = Context<Self>;
}

impl Handler<ClientMessage> for PubSubServer {
    type Result = Result<(), ClientError>;

    ///Implements processing of `ClientMessage`s for the `PubSubServer` actor
    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) -> Result<(), ClientError> {
        match msg.req {
            ClientRequest::List => {
                let sub_index_publication =
                    Publication::new(&serde_json::to_string_pretty(&self.subs.index())?);
                self.sessions
                    .get(&msg.id.id())
                    .ok_or(ClientError::InvalidInput(String::from("ClientID not found in sessions")))?
                    .do_send(sub_index_publication)
                    .map_err(|_e| {
                        ClientError::PublishingError(String::from(
                            "Failed sending subscription index",
                        ))
                    })
            }
            ClientRequest::Add { param } => match self.subs.fetch(&param) {
                Ok(mut s) => Ok(s.handle_subscribers(&msg.id, 0)),
                Err(e) => {
                    info!("{} :: Creating new subscription.", e);
                    let mut new_sub =
                        Subscription::new(&msg.id.id(), &format!("{}", param.to_simple()));
                    new_sub.handle_subscribers(&msg.id, 0);
                    Ok(self.subs.update(&new_sub))

                }
            },
            ClientRequest::Get { param } => {
                let s = self.subs.fetch(&param)?;
                let subscription_info = Publication::new(&serde_json::to_string_pretty(&s)?);
                self.sessions
                    .get(&msg.id.id())
                    .ok_or(ClientError::InvalidInput(String::from("ClientID not found in sessions")))?
                    .do_send(subscription_info)
                    .map_err(|_e| {
                        ClientError::PublishingError(String::from(
                            "Failed sending requested Subscription",
                        ))
                    })
            }
            ClientRequest::Publish { param } => {
                let s = self.subs.fetch(&param.0)?;
                let res = self.publish(&s.id, &param.1)?;
                let publication_result = Publication::new(&serde_json::to_string(&res)?);
                self.sessions
                    .get(&msg.id.id())
                    .ok_or(ClientError::InvalidInput(String::from("ClientID not found in sessions")))?
                    .do_send(publication_result)
                    .map_err(|_e| {
                        ClientError::PublishingError(String::from(
                            "Failed sending publication result",
                        ))
                    })
            }
            ClientRequest::Remove { param } => {
                Ok(self.subs.remove(&param))
            },
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::convert::TryFrom;
    use crate::protocol::ClientID;

    #[test]
    fn test_publication() {
        let test_pub = Publication::new(&String::from("test"));
        assert_eq!(
            Publication {
                id: Uuid::from(test_pub.id),
                data: String::from("test")
            },
            test_pub
        );
    }

    #[actix_rt::test]
    async fn test_pubsubserver_actor() {
        let server = PubSubServer::new().unwrap();
        let actor = server.start();
        assert_eq!(actor.connected(), true);
    }

    #[actix_rt::test]
    async fn test_pubsub_add_remove() {
        let server = PubSubServer::new().unwrap();
        let actor = &server.start();
        let sub_id = Uuid::new_v4();
        let mut sub = Subscription::new(&sub_id, "Test Subscription");
        let client_id = Uuid::new_v4();
        let client_msg =
            ClientMessage::try_from(format!("{}|add::{}", &client_id, &sub_id).as_str()).unwrap();
        actor.do_send(client_msg);
        sub.handle_subscribers(&ClientID::from(client_id) , 0);
    }
}
