use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use actix::prelude::{Actor, Context, Handler, Message, Recipient};

use crate::errors::ClientError;
use crate::protocol::{ClientMessage, ClientRequest};
use crate::subscription::{Subscription, SubscriptionMeta, Subscriptions};

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize, Message)]
#[rtype(result = "()")]
///Represents messages received and sent through `PubSubServer`
pub struct Publication {
    id: Uuid,
    data: String,
}

impl Publication {
    ///Creates a new `Publication` with a unique ID
    pub fn new(data: &String) -> Publication {
        Publication {
            id: Uuid::new_v4(),
            data: String::from(data),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
/// The actor managing `Subscriptions` and handling dissemination of `Publication`s.
/// Holds a list of currently connected sessions and a `Subscription` store.
pub struct PubSubServer {
    /// The subscription store
    subs: Subscriptions,
    /// Sessions are represented by the uid of a `ClientID` and
    /// a clients `WebSocketSession` address
    sessions: HashMap<Uuid, Recipient<Publication>>,
}

impl PubSubServer {
    /// Creates a new `PubSubServer` with a primary `Subscription`
    pub fn new() -> Result<PubSubServer, ClientError> {
        let mut subs = Subscriptions::new();
        let root_sub_meta = SubscriptionMeta {
            name: String::from("Subscriptions"),
        };
        let root_sub = Subscription::new(root_sub_meta)?;
        subs.update(&root_sub);
        Ok(PubSubServer {
            subs: subs,
            sessions: HashMap::new(),
        })
    }

    /// Sends a `Publication` to all subscribers of a `Subscription`
    fn publish(&self, sub_id: &Uuid, p: &Publication) -> Result<(), ClientError> {
        match self.subs.fetch(sub_id) {
            Ok(sub) => {
                let res = sub.subscribers.iter().map(|s| {
                    if let Some(recipient) = self.sessions.get(&s.id()) {
                        recipient.do_send(p.clone()).unwrap();
                    }
                });
                Ok(res.collect())
            }
            Err(e) => Err(ClientError::InvalidInput(format!("{}", e))),
        }
    }
}

impl Actor for PubSubServer {
    type Context = Context<Self>;
}

impl Handler<ClientMessage> for PubSubServer {
    type Result = Result<(), ClientError>;

    ///Implements processing of `ClientMessage`s for the `PubSubServer` actor
    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) -> Result<(), ClientError> {
        match msg.req {
            ClientRequest::Add { param } => match self.subs.fetch(&param) {
                Ok(mut s) => Ok(s.handle_subscribers(&msg.id, 0)),
                Err(e) => {
                    info!("{} :: Creating new subscription.", e);
                    let new_sub_meta = SubscriptionMeta {
                        name: format!("{}", msg.id),
                    };
                    let new_sub = Subscription::new(new_sub_meta)?;
                    Ok(self.subs.update(&new_sub))
                }
            },
            ClientRequest::Get { param } => {
                let s = self.subs.fetch(&param)?;
                let subscription_info = Publication::new(&serde_json::to_string_pretty(&s)?);
                self.sessions
                    .get(&msg.id.id())
                    .ok_or(ClientError::InvalidInput(String::from("Invalid ClientID")))?
                    .do_send(subscription_info)
                    .map_err(|_e| {
                        ClientError::PublishingError(String::from(
                            "Failed sending requested Subscription",
                        ))
                    })
            }
            ClientRequest::Publish { param } => {
                let s = self.subs.fetch(&param.0)?;
                Ok(self.publish(&s.id, &param.1)?)
            }
            ClientRequest::Remove { param } => {
                let mut s = self.subs.fetch(&param)?;
                let meta: SubscriptionMeta = serde_json::from_slice(&s.metadata)?;
                if meta.name == format!("{}", msg.id) {
                    Ok(self.subs.remove(&s.id))
                } else {
                    Ok(s.handle_subscribers(&msg.id, 1))
                }
            }
        }
    }
}
