use std::collections::{HashMap, HashSet};

use actix::prelude::{Actor, Addr, Context, Handler};
use uuid::Uuid;

use crate::data_log::DataLogger;
use crate::errors::ClientError;
use crate::messages::{
    ClientDisconnect, ClientJoin, ClientMessage, ClientRequest, ClientSubmission, Publication,
    Response, ServerMessage,
};
use crate::subscriptions::{Subscription, Subscriptions};
use crate::websocket::WebSocketSession;

/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
#[derive(Debug, PartialEq, Clone)]
pub struct PubSubServer {
    /// The subscription store
    subscriptions: Subscriptions,
    /// Sessions are represented by the uid of a `ClientID` and
    /// a clients `WebSocketSession` address
    sessions: HashMap<Uuid, Addr<WebSocketSession>>,
    /// An optionally available connection to a DataLogger actor
    data_logger: Option<Addr<DataLogger>>,
}

impl PubSubServer {
    /// Creates a new `PubSubServer`
    pub fn new() -> Result<PubSubServer, ClientError> {
        let subs = Subscriptions::new();
        Ok(PubSubServer {
            subscriptions: subs,
            sessions: HashMap::new(),
            data_logger: None,
        })
    }

    /// Enable persistent publication logs by providing a DataLogger actors Addr.
    pub fn with_data_log(&mut self, data_logger: Addr<DataLogger>) {
        self.data_logger = Some(data_logger.clone());
    }

    /// Retrieve a subscriptions log
    pub fn dump_log(&self, subscription_id: &Uuid) -> Result<HashSet<Uuid>, ClientError> {
        let sub = self.subscriptions.fetch(subscription_id)?;
        Ok(sub.log.clone())
    }

    /// Sends a `ServerMessageData::Response` to a `WebSocketSession` Actor
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
    fn publish(&mut self, submission: &ClientSubmission) -> Result<(), ClientError> {
        match self.subscriptions.fetch(&submission.id) {
            Ok(mut sub) => {
                let publication = Publication::from(submission);
                debug!("Logging publication for Subscription {}", &sub.id);
                if let Some(data_log) = &self.data_logger {
                    // TODO: this needs to be handled on a higher level. for now panicking
                    // here is fine. If data logging is enabled and fails, the error should be escalated
                    // immediately and any further data submissions by clients must be rejected until the situation is resolved.
                    sub.log_publication(&publication, data_log).unwrap();
                    self.subscriptions.update(&sub);
                }
                let publication = ServerMessage::from(&publication);
                info!("Distributing new publication for subscription {}", sub.id);
                Ok(sub.subscribers.iter().for_each(|s| {
                    if let Some(recipient) = self.sessions.get(&s) {
                        recipient.do_send(publication.clone())
                    }
                }))
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
        self.sessions.insert(join.id, join.addr);
    }
}

impl Handler<ClientDisconnect> for PubSubServer {
    type Result = ();

    fn handle(&mut self, disco: ClientDisconnect, _: &mut Context<Self>) {
        self.sessions.remove(&disco.id);
    }
}

impl Handler<ClientMessage> for PubSubServer {
    type Result = Result<(), ClientError>;

    ///Implements processing of `ClientMessage`s for the `PubSubServer` actor
    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) -> Result<(), ClientError> {
        let resp = match msg.request {
            ClientRequest::List => {
                debug!("Handling ClientRequest::List for {}", msg.id);
                Response::List {
                    data: self.subscriptions.index(),
                }
            }
            ClientRequest::Add { param } => {
                debug!(
                    "Handling ClientRequest::Add for {} with param {}",
                    msg.id, param
                );
                let resp_data = match self.subscriptions.fetch(&param) {
                    Ok(mut s) => {
                        s.append_subscriber(&msg.id);
                        self.subscriptions.update(&s);
                        s.id
                    }
                    Err(e) => {
                        info!("{} :: Creating new subscription.", e);
                        let mut new_sub = Subscription::new(&param, format!("{}", msg.id).as_str());
                        new_sub.append_subscriber(&msg.id);
                        self.subscriptions.update(&new_sub);
                        new_sub.id
                    }
                };
                Response::Add { data: resp_data }
            }
            ClientRequest::Get { param } => {
                debug!(
                    "Handling ClientRequest::Get for {} with param {}",
                    msg.id, param
                );
                Response::Get {
                    data: self.dump_log(&param)?,
                }
            }
            ClientRequest::Submit { param } => {
                debug!(
                    "Handling ClientRequest::Submit for {} with param {:#?}",
                    msg.id, param
                );
                self.publish(&param)?;
                Response::Empty
            }
            ClientRequest::Remove { param } => {
                debug!(
                    "Handling ClientRequest::Remove for {} with param {}",
                    msg.id, param
                );
                let resp_data = match self.subscriptions.fetch(&param) {
                    Ok(mut s) => {
                        s.remove_subscriber(&msg.id);
                        self.subscriptions.update(&s);
                        s.id
                    }
                    Err(e) => {
                        warn!("Could not remove subscription for {}, {}", &msg.id, e);
                        msg.id
                    }
                };
                Response::Remove { data: resp_data }
            }
        };
        Ok(self.send_response(&msg.id, &resp))
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
        server.subscriptions.update(&subscription);
        let dummy_submission = ClientSubmission {
            id: sub_id,
            data: serde_cbor::to_vec(&String::from("Test")).unwrap(),
        };
        assert!(server.publish(&dummy_submission).is_ok());
    }

    #[actix_rt::test]
    async fn test_start_pubsub() {
        let dummy_server = PubSubServer::new().unwrap();
        let pubsub = dummy_server.start();
        assert_eq!(pubsub.connected(), true);
    }
}
