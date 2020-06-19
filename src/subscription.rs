use std::collections::HashMap;

use actix_web::error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;

///Holds Subscription specific information
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct SubscriptionMeta {
    pub name: String,
}

/// Represents an entry in `crate::subscription::Subscriptions`
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Identifier
    pub id: Uuid,
    /// An array representation of a serialized `crate::subscription::SubscriptionMeta`
    pub metadata: Vec<u8>,
}

impl Subscription {
    /// Performs serialization of `crate::subscription::SubscriptionMeta` and
    /// creates `crate::subscription::Subscription`
    pub fn new(meta: SubscriptionMeta) -> Result<Subscription, ClientError> {
        let meta_vec = serde_json::to_vec(&meta)?;
        Ok(Subscription {
            id: Uuid::new_v4(),
            metadata: meta_vec,
        })
    }
}

impl From<(&Uuid, &Vec<u8>)> for Subscription {
    /// Convert from entries in `crate::subscription::Subscriptions`
    fn from(entry: (&Uuid, &Vec<u8>)) -> Subscription {
        Subscription {
            id: entry.0.to_owned(),
            metadata: entry.1.to_owned()}
    }
}

/// Holds the subscription store
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscriptions {
    store: Box<HashMap<Uuid, Vec<u8>>>,
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
        self.store.insert(sub.id, sub.metadata.clone());
    }

    /// Attempts to retrieve a `crate::subscription::Subscription` from the subscription store
    pub fn fetch(&self, id: &Uuid) -> Result<Subscription, error::Error> {
        if let Some(sub) = self.store.get_key_value(id) {
            Ok(Subscription::from(sub))
        } else {
            Err(error::ErrorNotFound("No such entry"))
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
    fn test_subscription () {
        let dummy_data = serde_json::to_vec(&SubscriptionMeta{ name: "Test Subscription".to_owned() }).unwrap();
        let sub_meta = SubscriptionMeta{ name: "Test Subscription".to_owned() };
        let sub = Subscription::new(sub_meta).unwrap();
        assert_eq!(dummy_data, sub.metadata);
    }

    #[test]
    fn test_subscriptions () {
        let mut subscriptions = Subscriptions::new();
        let subscription = Subscription::new(
            SubscriptionMeta{ name: "Test".to_owned()}
        ).unwrap();
        subscriptions.update(&subscription);
        let fetched_subscription = subscriptions.fetch(&subscription.id).unwrap().to_owned();
        assert_eq!(fetched_subscription, subscription);
        subscriptions.remove(&fetched_subscription.id);
        let subscription_index = &subscriptions.index();
        assert_eq!(subscription_index, &Vec::<&Uuid>::new())
    }
}