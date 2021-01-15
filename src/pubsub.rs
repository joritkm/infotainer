use std::collections::{HashMap, HashSet};

use actix::prelude::{Actor, Addr, Context, Handler, Message, Recipient, ResponseFuture};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    data_log::{DataLogEntry, DataLogPut, DataLogger},
    prelude::ServerMessage,
    sessions::{GetSessionAddr, SessionError},
    websocket::ClientError,
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
    Subscribing(String),
}

impl From<SessionError> for PublicationError {
    fn from(e: SessionError) -> Self {
        Self::SessionService(format!("{}", e))
    }
}

/// A message to add or remove a client id from a subscription
#[derive(Debug, Clone, Message)]
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

/// Represents data submitted by the client for publication to a Subscription.
#[derive(Debug, Clone, Message)]
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

/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
#[derive(Debug, Clone)]
pub struct PubSubService {
    /// The subscription store
    subscriptions: Subscriptions,
    sessions: Recipient<GetSessionAddr>,
    /// An optionally available connection to a DataLogger actor
    datalog: Addr<DataLogger>,
}

impl Actor for PubSubService {
    type Context = Context<Self>;
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
                match self.subscriptions.fetch(&subscription_id) {
                    Ok(mut s) => {
                        s.remove_subscriber(&client_id);
                        Ok(if s.subscribers.is_empty() {
                            self.subscriptions.remove(&subscription_id)
                        } else {
                            self.subscriptions.update(&s)
                        })
                    }
                    Err(e) => {
                        warn!("Could not remove subscription for {}, {}", &client_id, e);
                        Err(PublicationError::Subscribing(format!(
                            "Could not remove subscription {}",
                            e
                        )))
                    }
                }
            }
        }
    }
}

impl Handler<SubmitCommand> for PubSubService {
    type Result = ResponseFuture<Result<(), PublicationError>>;

    fn handle(&mut self, subcmd: SubmitCommand, _: &mut Context<Self>) -> Self::Result {
        debug!(" {} submitted {:?}", subcmd.client_id, subcmd.submission);
        match self.subscriptions.fetch(&subcmd.subscription_id) {
            Ok(sub) => {
                let publication = Publication::new(&subcmd.subscription_id, &subcmd.submission);
                let session_service = self.sessions.clone();
                let data_log_service = self.datalog.clone();
                let issue = ServerMessage::from(&publication);
                info!("Distributing new publication for subscription {}", sub.id);
                Box::pin(async move {
                    debug!("Logging publication for Subscription {}", &sub.id);
                    data_log_service
                        .try_send(DataLogPut::new(&sub.id, DataLogEntry::from(&publication)))
                        .map_err(|e| PublicationError::DataLoggingError(format!("{}", e)))?;
                    for s in sub.subscribers {
                        if let Ok(session_result) =
                            session_service.send(GetSessionAddr::from(&s)).await
                        {
                            if let Ok(recipient) = session_result {
                                if let Ok(publishing_result) = recipient.send(issue.clone()).await {
                                    if let Err(e) = publishing_result {
                                        error!(
                                            "Error while delivering Publication to {}: {}",
                                            &s, e
                                        );
                                    }
                                } else {
                                    error!("Error while sending publication to client {}", &s);
                                }
                            } else {
                                error!("Error retrieving session for client {}", &s);
                            }
                        }
                    }
                    Ok(())
                })
            }
            Err(e) => Box::pin(async move { Err(PublicationError::Publishing(format!("{}", e))) }),
        }
    }
}

impl PubSubService {
    /// Creates a new `PubSubService` actor.
    pub fn new(datalog: &Addr<DataLogger>, sessions: &Recipient<GetSessionAddr>) -> Self {
        let subs = Subscriptions::new();
        PubSubService {
            subscriptions: subs,
            sessions: sessions.clone(),
            datalog: datalog.clone(),
        }
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
        assert!(subscriptions.fetch(&fetched_subscription.id).is_err())
    }
}
