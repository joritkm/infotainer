use std::collections::{HashMap, HashSet};

use actix::prelude::{Actor, Addr, Context, Handler, Message};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::data_log::{DataLogEntry, DataLogError, DataLogRequest, DataLogger};
use crate::websocket::{
    ClientDisconnect, ClientError, ClientJoin, ClientMessage, ClientRequest, ClientSubmission,
    WebSocketSession,
};

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum PublicationError {
    #[fail(display = "Could not log publication to the data log: {}", _0)]
    DataLoggingError(String),
}

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

impl Default for PubSubServer {
    fn default() -> PubSubServer {
        let subs = Subscriptions::new();
        PubSubServer {
            subscriptions: subs,
            sessions: HashMap::new(),
            data_logger: None,
        }
    }
}

impl PubSubServer {
    /// Creates a new `PubSubServer`. Providing a DataLoggers Addr will enable persistent logging.
    pub fn new(data_logger: Option<&Addr<DataLogger>>) -> Result<PubSubServer, ClientError> {
        let subs = Subscriptions::new();
        Ok(PubSubServer {
            subscriptions: subs,
            sessions: HashMap::new(),
            data_logger: data_logger.cloned(),
        })
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

/// Represents an accepted Submission that can be stored and distributed
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Publication {
    pub id: Uuid,
    pub data: Vec<u8>,
}

impl From<&ClientSubmission> for Publication {
    fn from(submission: &ClientSubmission) -> Self {
        Publication {
            id: Uuid::new_v4(),
            data: submission.data.clone(),
        }
    }
}

///Holds Subscription specific information
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct SubscriptionMeta {
    /// The name of the subscription
    pub name: String,
}

/// Represents an entry in `crate::subscription::Subscriptions`
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Identifier, corresponds to the id of the creating client
    pub id: Uuid,
    /// Metadata represented by `SubscriptionMeta`
    pub metadata: SubscriptionMeta,
    /// List of currently subscribed clients
    pub subscribers: Vec<Uuid>,
    /// Log of published messages
    pub log: HashSet<Uuid>,
}

impl Subscription {
    /// Generates metadata and creates a new `Subscription`
    pub fn new(id: &Uuid, name: &str) -> Subscription {
        let meta = SubscriptionMeta {
            name: name.to_owned(),
        };
        Subscription {
            id: *id,
            metadata: meta,
            subscribers: Vec::new(),
            log: HashSet::new(),
        }
    }

    /// Appends a new subscriber to the subscribers Array
    pub fn append_subscriber(&mut self, subscriber: &Uuid) {
        if !self.subscribers.contains(subscriber) {
            self.subscribers.push(*subscriber)
        }
    }

    /// Removes a subscriber from the subscribers Array
    pub fn remove_subscriber(&mut self, subscriber: &Uuid) {
        if let Some(sub_index) = self.subscribers.iter().position(|s| s == subscriber) {
            self.subscribers.remove(sub_index);
        }
    }

    /// Appends a submitted publication to the log
    pub fn log_publication(
        &mut self,
        publication: &Publication,
        data_log: &Addr<DataLogger>,
    ) -> Result<bool, DataLogError> {
        data_log.try_send(DataLogRequest::new(
            &self.id,
            DataLogEntry::from(publication),
        ))?;
        Ok(self.log.insert(publication.id))
    }
}

/// Holds the subscription store. Subscriptions are stored
/// in a HashMap, identified by their id.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscriptions {
    store: Box<HashMap<Uuid, Subscription>>,
}

impl Subscriptions {
    /// Initialize a new subscription store
    pub fn new() -> Subscriptions {
        Subscriptions {
            store: Box::new(HashMap::new()),
        }
    }

    /// Retrieves a list containing the identifying Uuids of all available Subscriptions
    pub fn index(&self) -> Vec<Uuid> {
        self.store.keys().map(|i| *i).collect()
    }

    /// Updates the subscription store with new entries,
    /// silently replacing existing ones
    pub fn update(&mut self, sub: &Subscription) {
        self.store.insert(sub.id, sub.clone());
    }

    /// Attempts to retrieve a `crate::subscription::Subscription` from the subscription store
    pub fn fetch(&self, id: &Uuid) -> Result<Subscription, ClientError> {
        if let Some(sub) = self.store.get_key_value(id) {
            Ok(sub.1.clone())
        } else {
            Err(ClientError::InvalidInput(String::from("No such entry")))
        }
    }

    /// Removes a subscription from the subscription store
    pub fn remove(&mut self, id: &Uuid) {
        self.store.remove(id);
    }
}

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

/// Represents data sent by the server in response to a ClientRequest
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum Response {
    List { data: Vec<Uuid> },
    Add { data: Uuid },
    Get { data: HashSet<Uuid> },
    Remove { data: Uuid },
    Empty,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_submitting_publication() {
        let mut server = PubSubServer::default();
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
        let dummy_server = PubSubServer::default();
        let pubsub = dummy_server.start();
        assert_eq!(pubsub.connected(), true);
    }

    #[test]
    fn test_subscription() {
        let dummy_client = Uuid::new_v4();
        let mut dummy_subscription = Subscription::new(&dummy_client, "Test Subscription");

        assert_eq!(
            dummy_subscription.metadata,
            SubscriptionMeta {
                name: String::from("Test Subscription")
            }
        );

        dummy_subscription.append_subscriber(&dummy_client);
        assert!(dummy_subscription.subscribers.contains(&dummy_client));
        dummy_subscription.remove_subscriber(&dummy_client);
        assert_eq!(
            dummy_subscription.subscribers.contains(&dummy_client),
            false
        );
    }

    #[test]
    fn test_subscriptions() {
        let mut subscriptions = Subscriptions::new();
        let subscription = Subscription::new(&Uuid::new_v4(), "Test Subscription");
        subscriptions.update(&subscription);
        let fetched_subscription = subscriptions.fetch(&subscription.id).unwrap().to_owned();
        assert_eq!(fetched_subscription, subscription);
        subscriptions.remove(&fetched_subscription.id);
        let subscription_index = &subscriptions.index();
        assert_eq!(subscription_index, &Vec::<Uuid>::new())
    }
}
