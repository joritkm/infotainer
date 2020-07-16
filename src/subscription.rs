use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;
use crate::protocol::ClientID;

///Holds Subscription specific information
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct SubscriptionMeta {
    /// The name of the subscription
    pub name: String,
}

/// Represents an entry in `crate::subscription::Subscriptions`
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Identifier
    pub id: Uuid,
    /// An array representation of a serialized `crate::subscription::SubscriptionMeta`
    pub metadata: Vec<u8>,
    /// The list of currently subscribed clients
    pub subscribers: Vec<ClientID>,
}

impl Subscription {
    /// Performs serialization of `crate::subscription::SubscriptionMeta` and
    /// creates `crate::subscription::Subscription`
    pub fn new(meta: SubscriptionMeta) -> Result<Subscription, ClientError> {
        let meta_vec = serde_json::to_vec(&meta)?;
        Ok(Subscription {
            id: Uuid::new_v4(),
            metadata: meta_vec,
            subscribers: Vec::new(),
        })
    }

    /// Appends a new subscriber to the subscribers Array
    fn append_subscriber(&mut self, subscriber: &ClientID) {
        if !self.subscribers.contains(subscriber) {
            self.subscribers.push(subscriber.to_owned())
        }
    }
    /// Removes a subscriber from the subscribers Array
    fn remove_subscriber(&mut self, subscriber: &ClientID) {
        if let Some(sub_index) = self.subscribers.iter().position(|s| s == subscriber) {
            self.subscribers.remove(sub_index);
        }
    }

    /// Handles removal and addition of subscribers. Any non-zero value
    /// for action will attempt to remove ClientID from subscribers.
    pub fn handle_subscribers(&mut self, client: &ClientID, action: usize) {
        match action {
            0 => self.append_subscriber(client),
            _ => self.remove_subscriber(client),
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

    /// Retrieves a list of the current subscriptions
    pub fn index(&self) -> Vec<&Uuid> {
        self.store.keys().collect()
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
        self.store.remove(&id);
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_subscription() {
        let dummy_client = ClientID::from(Uuid::new_v4());
        let dummy_meta = SubscriptionMeta {
            name: String::from("Test Subscription"),
        };
        let mut dummy_subscription = Subscription::new(dummy_meta.clone()).unwrap();

        assert_eq!(
            dummy_subscription.metadata,
            serde_json::to_vec(&dummy_meta).unwrap()
        );

        dummy_subscription.handle_subscribers(&dummy_client, 0);
        assert_eq!(dummy_subscription.subscribers[0], dummy_client);

        dummy_subscription.handle_subscribers(&dummy_client, 1);
        assert_eq!(dummy_subscription.subscribers, Vec::<ClientID>::new())
    }

    #[test]
    fn test_subscriptions() {
        let mut subscriptions = Subscriptions::new();
        let subscription = Subscription::new(SubscriptionMeta {
            name: "Test".to_owned(),
        })
        .unwrap();
        subscriptions.update(&subscription);
        let fetched_subscription = subscriptions.fetch(&subscription.id).unwrap().to_owned();
        assert_eq!(fetched_subscription, subscription);
        subscriptions.remove(&fetched_subscription.id);
        let subscription_index = &subscriptions.index();
        assert_eq!(subscription_index, &Vec::<&Uuid>::new())
    }
}
