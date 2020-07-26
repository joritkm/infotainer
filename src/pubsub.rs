use std::collections::HashMap;
use uuid::Uuid;

use actix::prelude::{Actor, Context, Handler, Recipient};

use crate::errors::ClientError;
use crate::protocol::{
    ClientDisconnect, ClientJoin, ClientMessage, ClientRequest, ClientSubmission, ServerMessage,
    ServerMessageData,
};
use crate::subscription::{Subscription, Subscriptions};

/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
#[derive(Debug, PartialEq, Clone)]
pub struct PubSubServer {
    /// The subscription store
    subs: Subscriptions,
    /// Sessions are represented by the uid of a `ClientID` and
    /// a clients `WebSocketSession` address
    sessions: HashMap<Uuid, Recipient<ServerMessage<ServerMessageData>>>,
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

    /// Sends a `ServerMessageData::Response` to a connected client
    fn send_message(&self, client_id: Uuid, msg: &str) {
        debug!("Attempting to send message {} to {}", msg, client_id);
        if let Some(session) = self.sessions.get(&client_id) {
            let msg = ServerMessage {
                content: ServerMessageData::from(&msg.to_owned()),
            };
            session.do_send(msg).ok();
        } else {
            info!("Could not send message to {}. The session could not be found.", client_id)
        }
    }

    /// Publishes a `ClientSubmission` to all subscribers of a `Subscription`
    fn publish(&self, submission: &ClientSubmission) -> Result<Vec<String>, ClientError> {
        match self.subs.fetch(&submission.id) {
            Ok(sub) => {
                let publication = ServerMessageData::from(submission);
                let message = ServerMessage {
                    content: publication,
                };
                info!("Distributing new publication for subscription {}", sub.id);
                let res = sub
                    .subscribers
                    .iter()
                    .map(|s| match self.sessions.get(&s.id()) {
                        Some(recipient) => {
                            recipient.do_send(message.clone()).unwrap();
                            debug!("Publication sent to {}", &s);
                            format!("{}", &s.id().to_hyphenated())
                        }
                        _ => format!("No session found for ClientID: {}", &s.id()),
                    })
                    .collect();
                Ok(res)
            }
            Err(e) => Err(ClientError::InvalidInput(format!("{}", e))),
        }
    }
}

impl Actor for PubSubServer {
    type Context = Context<Self>;
}

impl Handler<ClientJoin> for PubSubServer {
    type Result = ();

    fn handle(&mut self, join: ClientJoin, _: &mut Context<Self>) {
        self.sessions.insert(
            join.id.id(),
            join.addr.recipient::<ServerMessage<ServerMessageData>>(),
        );
        self.send_message(join.id.id(), format!("{}", join.id).as_str())
    }
}

impl Handler<ClientDisconnect> for PubSubServer {
    type Result = ();

    fn handle(&mut self, disco: ClientDisconnect, _: &mut Context<Self>) {
        self.sessions.remove(&disco.id.id());
    }
}

impl Handler<ClientMessage> for PubSubServer {
    type Result = Result<(), ClientError>;

    ///Implements processing of `ClientMessage`s for the `PubSubServer` actor
    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) -> Result<(), ClientError> {
        match msg.req {
            ClientRequest::List => {
                debug!("Handling ClientRequest::List for {}", msg.id);
                Ok(self.send_message(
                    msg.id.id(),
                    &serde_json::to_string_pretty(&self.subs.index())?,
                ))
            },
            ClientRequest::Add { param } => {
                debug!("Handling ClientRequest::Add for {} with param {}", msg.id, param);
                match self.subs.fetch(&param) {
                    Ok(mut s) => {
                        s.handle_subscribers(&msg.id, 0);
                        Ok(self.subs.update(&s))
                    },
                    Err(e) => {
                        info!("{} :: Creating new subscription.", e);
                        let mut new_sub = Subscription::new(&param, format!("{}", msg.id).as_str());
                        new_sub.handle_subscribers(&msg.id, 0);
                        Ok(self.subs.update(&new_sub))
                    }
                }
            },
            ClientRequest::Get { param } => {
                debug!("Handling ClientRequest::Get for {} with param {}", msg.id, param);
                let s = self.subs.fetch(&param)?;
                Ok(self.send_message(msg.id.id(), &serde_json::to_string_pretty(&s)?))
            }
            ClientRequest::Publish { param } => {
                debug!("Handling ClientRequest::Publish for {} with param {:#?}", msg.id, param);
                let res = self.publish(&param)?;
                Ok(self.send_message(msg.id.id(), &serde_json::to_string(&res)?))
            }
            ClientRequest::Remove { param } => {
                debug!("Handling ClientRequest::Remove for {} with param {}", msg.id, param);
                Ok(self.subs.remove(&param))
            },
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::protocol::ClientID;
    use std::convert::TryFrom;

    #[test]
    fn test_submitting_publication() {
        let mut server = PubSubServer::new().unwrap();
        let sub_id = Uuid::new_v4();
        let subscription = Subscription::new(&sub_id, "Test");
        server.subs.update(&subscription);
        let dummy_submission = ClientSubmission {id: sub_id, data: "Test".to_owned()};
        let published = server.publish(&dummy_submission).unwrap();
        assert_eq!(published, Vec::<String>::new())
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
        sub.handle_subscribers(&ClientID::from(client_id), 0);
    }
}
