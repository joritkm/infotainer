use std::collections::{HashMap, HashSet};

use actix::Addr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::data_log::{DataLogEntry, DataLogRequest, DataLogger};
use crate::errors::{ClientError, DataLogError};
use crate::messages::Publication;

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
        let subscription_index = &subscriptions.index();
        assert_eq!(subscription_index, &Vec::<Uuid>::new())
    }
}
