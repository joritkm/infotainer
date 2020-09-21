use std::collections::HashMap;
use uuid::Uuid;

use actix::prelude::{Actor, Addr, Context, Handler};

use crate::errors::ClientError;
use crate::protocol::{
    ClientDisconnect, ClientJoin, ClientMessage, ClientRequest, Publication, Response,
    ServerMessage,
};
use crate::subscription::{Subscription, Subscriptions};
use crate::websocket::WebSocketSession;

/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
#[derive(Debug, PartialEq, Clone)]
pub struct PubSubServer {
    /// The subscription store
    subs: Subscriptions,
    /// Sessions are represented by the uid of a `ClientID` and
    /// a clients `WebSocketSession` address
    sessions: HashMap<Uuid, Addr<WebSocketSession>>,
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
    /// TODO: implement message id
    fn send_response(&self, client_id: &Uuid, resp: &Response) {
        debug!("Attempting to send reponse {:?}", resp);
        if let Some(session) = self.sessions.get(client_id) {
            let msg = ServerMessage::from(resp);
            session.do_send(msg);
        } else {
            info!(
                "Could not send message to {}. The session could not be found.",
                client_id
            )
        }
    }

    /// Publishes a `ClientSubmission` to all subscribers of a `Subscription`
    fn publish(&self, submission: &Publication) -> Result<Vec<String>, ClientError> {
        match self.subs.fetch(&submission.id) {
            Ok(sub) => {
                let publication = ServerMessage::from(submission);
                info!("Distributing new publication for subscription {}", sub.id);
                let res = sub
                    .subscribers
                    .iter()
                    .map(|s| match self.sessions.get(&s.id()) {
                        Some(recipient) => {
                            recipient.do_send(publication.clone());
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
        self.sessions.insert(join.id.id(), join.addr);
        let resp = Response {
            msg_id: Uuid::nil(),
            data: format!("{}", join.id),
        };
        self.send_response(&join.id.id(), &resp);
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
        let resp = match msg.request {
            ClientRequest::List => {
                debug!("Handling ClientRequest::List for {}", msg.id);
                Response {
                    msg_id: msg.msg_id,
                    data: serde_json::to_string(&self.subs.index())?,
                }
            }
            ClientRequest::Add { param } => {
                debug!(
                    "Handling ClientRequest::Add for {} with param {}",
                    msg.id, param
                );
                let resp_data = match self.subs.fetch(&param) {
                    Ok(mut s) => {
                        s.append_subscriber(&msg.id);
                        self.subs.update(&s);
                        format!("Subscribed to {}", &s.id)
                    }
                    Err(e) => {
                        info!("{} :: Creating new subscription.", e);
                        let mut new_sub = Subscription::new(&param, format!("{}", msg.id).as_str());
                        new_sub.append_subscriber(&msg.id);
                        self.subs.update(&new_sub);
                        format!(
                            "Created and subscribed to new Subscription {}",
                            &new_sub.id
                        )
                    }
                };
                Response {
                    msg_id: msg.msg_id,
                    data: resp_data,
                }
            }
            ClientRequest::Get { param } => {
                debug!(
                    "Handling ClientRequest::Get for {} with param {}",
                    msg.id, param
                );
                let s = self.subs.fetch(&param)?;
                Response {
                    msg_id: msg.id.id(),
                    data: serde_json::to_string(&s)?,
                }
            }
            ClientRequest::Publish { param } => {
                debug!(
                    "Handling ClientRequest::Publish for {} with param {:#?}",
                    msg.id, param
                );
                let res = self.publish(&param)?;
                Response {
                    msg_id: msg.id.id(),
                    data: serde_json::to_string(&res)?,
                }
            }
            ClientRequest::Remove { param } => {
                debug!(
                    "Handling ClientRequest::Remove for {} with param {}",
                    msg.id, param
                );
                let resp_data = match self.subs.fetch(&param) {
                    Ok(mut s) => {
                        s.remove_subscriber(&msg.id);
                        self.subs.update(&s);
                        format!("Unsubscribed from {}", &s.id)
                    }
                    Err(e) => {
                        warn!("Could not remove subscription for {}, {}", &msg.id, e);
                        format!("Could not unsubscribe {} from {}. Client not subscribed", &msg.id, &param)
                    }
                };
                Response {
                    msg_id: msg.id.id(),
                    data: resp_data,
                }
            }
        };
        Ok(self.send_response(&msg.id.id(), &resp))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_submitting_publication() {
        let mut server = PubSubServer::new().unwrap();
        let sub_id = Uuid::new_v4();
        let subscription = Subscription::new(&sub_id, "Test");
        server.subs.update(&subscription);
        let dummy_submission = Publication {
            id: sub_id,
            data: "Test".to_owned(),
        };
        let published = server.publish(&dummy_submission).unwrap();
        assert_eq!(published, Vec::<String>::new())
    }

    #[test]
    fn test_sending_server_response() {
        let server = PubSubServer::new().unwrap();
        let dummy_client_id = Uuid::new_v4();
        let dummy_message_id = Uuid::nil();
        let resp = Response {
            msg_id: dummy_message_id,
            data: "Test".to_owned(),
        };
        server.send_response(&dummy_client_id, &resp);
    }

    #[actix_rt::test]
    async fn test_start_pubsub() {
        let dummy_server = PubSubServer::new().unwrap();
        let pubsub = dummy_server.start();
        assert_eq!(pubsub.connected(), true);
    }
}
