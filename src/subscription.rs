use std::collections::HashMap;

use actix_web::error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ClientError;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct SubscriptionMeta {
    name: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscription {
    id: Uuid,
    metadata: Vec<u8>,
}

impl Subscription {
    pub fn new(raw: SubscriptionMeta) -> Result<Subscription, ClientError> {
        let meta = serde_json::to_vec(&raw)?;
        Ok(Subscription {
            id: Uuid::new_v4(),
            metadata: meta,
        })
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Subscriptions {
    store: HashMap<Uuid, Box<Subscription>>,
}

impl Subscriptions {
    pub fn new() -> Subscriptions {
        Subscriptions {
            store: HashMap::new(),
        }
    }

    pub fn update(mut self, sub: Subscription) {
        self.store.insert(sub.id, Box::new(sub));
    }

    pub fn fetch(&self, id: &Uuid) -> Result<&Box<Subscription>, error::Error> {
        self.store
            .get(&id)
            .ok_or(error::ErrorNotFound("No such entry"))
    }

    pub fn remove(mut self, id: &Uuid) {
        self.store.remove(&id);
    }
}