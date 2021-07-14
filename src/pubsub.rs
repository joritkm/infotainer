use std::collections::{HashMap, HashSet};

use actix::{
    prelude::{Actor, Context, Handler, Message},
    Addr,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    data_log::{DataLogPut, DataLogger},
    websocket::WebSocketSession,
};

/// Represents errors caused during interaction with the PubSubService actor
#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum PublicationError {
    #[fail(display = "Could not log publication to the data log: {}", _0)]
    DataLoggingError(String),

    #[fail(display = "Error while communicating with SessionService: {}", _0)]
    SessionService(String),

    #[fail(display = "Error while publishing: {}", _0)]
    Publishing(String),

    #[fail(display = "Error while handling subscriptions: {}", _0)]
    Subscriptions(&'static str),
}

/// A message to register a websocket session with the pubsub service
#[derive(Debug, Message)]
#[rtype("Result<(), PublicationError>")]
pub enum ManageSession {
    /// Add [Addr] of clients [WebsocketSession] to [PubSubService.sessions]
    Add {
        client_id: Uuid,
        addr: Addr<WebSocketSession>,
    },
    /// Remove websocket session from [PubSubService.sessions]
    Remove { client_id: Uuid },
}

/// A message to add or remove a client id from a subscription
#[derive(Debug, Message)]
#[rtype("Result<(), PublicationError>")]
pub enum ManageSubscription {
    /// Add client to a Subscription, creating it, if it doesn't exist
    Add {
        client_id: Uuid,
        subscription_id: Uuid,
    },
    /// Clients _are_ allowed to cancel their Subscription
    Remove {
        client_id: Uuid,
        subscription_id: Uuid,
    },
}

/// A message to submit data for publishing
#[derive(Debug, Message)]
#[rtype(result = "Result<(), PublicationError>")]
pub struct SubmitCommand {
    client_id: Uuid,
    subscription_id: Uuid,
    submission: Vec<u8>,
}

impl SubmitCommand {
    pub fn new(client: &Uuid, subscription_id: &Uuid, submission: &Vec<u8>) -> Self {
        SubmitCommand {
            client_id: client.clone(),
            subscription_id: subscription_id.clone(),
            submission: submission.clone(),
        }
    }
}

/// A message informing clients about newly submitted publications
#[derive(Debug, Deserialize, Message, Serialize)]
#[rtype("Result<(), PublicationError>")]
pub struct Issue(pub Uuid, pub Uuid);

/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
#[derive(Debug, Clone)]
pub struct PubSubService {
    subscriptions: Subscriptions,
    sessions: HashMap<Uuid, Addr<WebSocketSession>>,
    data_log_addr: Addr<DataLogger>,
}

impl PubSubService {
    /// Creates a new `PubSubService` actor.
    pub fn new(data_log_addr: &Addr<DataLogger>) -> Self {
        let subs = Subscriptions::new();
        PubSubService {
            subscriptions: subs,
            sessions: HashMap::new(),
            data_log_addr: data_log_addr.clone(),
        }
    }
}

impl Actor for PubSubService {
    type Context = Context<Self>;
}

impl Handler<ManageSession> for PubSubService {
    type Result = Result<(), PublicationError>;

    fn handle(&mut self, msg: ManageSession, _: &mut Self::Context) -> Self::Result {
        Ok(match msg {
            ManageSession::Add { client_id, addr } => {
                self.sessions.insert(client_id, addr);
            }
            ManageSession::Remove { client_id } => {
                self.sessions.remove(&client_id);
            }
        })
    }
}

impl Handler<ManageSubscription> for PubSubService {
    type Result = Result<(), PublicationError>;

    fn handle(&mut self, subcmd: ManageSubscription, _: &mut Context<Self>) -> Self::Result {
        match subcmd {
            ManageSubscription::Add {
                client_id,
                subscription_id,
            } => {
                debug!(
                    "Handling SubscriptionCommand::Add for {} with param {}",
                    &client_id, &subscription_id
                );
                Ok(match self.subscriptions.fetch(&subscription_id) {
                    Ok(mut s) => {
                        s.append_subscriber(&client_id);
                        self.subscriptions.update(&s);
                    }
                    Err(e) => {
                        info!("{} :: Creating new subscription.", e);
                        let mut new_sub =
                            Subscription::new(&subscription_id, format!("{}", &client_id).as_str());
                        new_sub.append_subscriber(&client_id);
                        self.subscriptions.update(&new_sub);
                    }
                })
            }
            ManageSubscription::Remove {
                client_id,
                subscription_id,
            } => {
                debug!(
                    "Handling SubscriptionCommand::Remove for {} with param {}",
                    &client_id, &subscription_id
                );
                let mut s = self.subscriptions.fetch(&subscription_id)?;
                s.remove_subscriber(&client_id);
                Ok(if s.subscribers.is_empty() {
                    self.subscriptions.remove(&subscription_id)
                } else {
                    self.subscriptions.update(&s)
                })
            }
        }
    }
}

impl Handler<SubmitCommand> for PubSubService {
    type Result = Result<(), PublicationError>;

    fn handle(&mut self, msg: SubmitCommand, _: &mut Context<Self>) -> Self::Result {
        debug!(" {} submitted {:?}", msg.client_id, msg.submission);
        Ok(
            if let Ok(subscription) = self.subscriptions.fetch(&msg.subscription_id) {
                let publication = Publication::new(&msg.subscription_id, &msg.submission);
                self.data_log_addr
                    .try_send(DataLogPut(vec![publication.clone()]))
                    .map_err(|e| {
                        PublicationError::DataLoggingError(format!(
                            "Could not write published message to datalog: {}",
                            e.to_string()
                        ))
                    })?;
                for s in subscription.subscribers {
                    if let Some(recipient) = self.sessions.get(&s) {
                        recipient
                            .try_send(Issue(subscription.id, publication.publication_id))
                            .map_err(|e| PublicationError::Publishing(e.to_string()))?;
                    }
                }
            },
        )
    }
}

/// Represents an accepted Submission that can be stored and distributed
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Publication {
    pub publication_id: Uuid,
    pub subscription_id: Uuid,
    pub data: Vec<u8>,
}

impl Publication {
    fn new(subscription_id: &Uuid, data: &Vec<u8>) -> Self {
        Publication {
            publication_id: Uuid::new_v4(),
            subscription_id: *subscription_id,
            data: data.clone(),
        }
    }
}

/// Represents an entry in `crate::subscription::Subscriptions`
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Unique identifier
    pub id: Uuid,
    /// Descriptive name
    pub name: String,
    /// List of currently subscribed clients
    pub subscribers: Vec<Uuid>,
}

impl Subscription {
    /// Generates metadata and creates a new `Subscription`
    pub fn new(id: &Uuid, name: &str) -> Subscription {
        Subscription {
            id: *id,
            name: name.to_owned(),
            subscribers: Vec::new(),
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

    /// Updates the subscription store with new entries,
    /// silently replacing existing ones
    pub fn update(&mut self, sub: &Subscription) {
        self.store.insert(sub.id, sub.clone());
    }

    /// Attempts to retrieve a `crate::subscription::Subscription` from the subscription store
    pub fn fetch(&self, id: &Uuid) -> Result<Subscription, PublicationError> {
        self.store
            .get(id)
            .ok_or(PublicationError::Subscriptions("Subscription not found"))
            .and_then(|s| Ok(s.to_owned()))
    }

    /// Removes a subscription from the subscription store
    pub fn remove(&mut self, id: &Uuid) {
        self.store.remove(id);
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
    fn test_subscription() {
        let dummy_client = Uuid::new_v4();
        let mut dummy_subscription = Subscription::new(&dummy_client, "Test Subscription");

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
        assert!(subscriptions.fetch(&fetched_subscription.id).is_err())
    }
}
