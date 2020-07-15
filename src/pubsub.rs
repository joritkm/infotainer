use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use actix::prelude::{Actor, Context, Recipient, Message, Handler};

use crate::protocol::{ClientID, ClientMessage, ClientRequest::*, ClientRequest};
use crate::errors::ClientError;
use crate::subscription::{Subscription, Subscriptions, SubscriptionMeta};


#[derive(Debug, PartialEq, Clone, Deserialize, Serialize, Message)]
#[rtype(result = "()")]
pub struct Publication {
    id: Uuid,
    data: String
}


impl Publication {
    pub fn new(data: &String) -> Publication {
        Publication {
            id: Uuid::new_v4(),
            data: String::from(data)
        }
    }
}


#[derive(Debug, PartialEq, Clone)]
pub struct PubSubServer {
    subs: Subscriptions,
    sessions: HashMap<Uuid, Recipient<Publication>>,
}

impl PubSubServer {
    pub fn new() -> Result<PubSubServer, ClientError> {
        let mut subs = Subscriptions::new();
        let root_sub_meta = SubscriptionMeta { name: String::from("Subscriptions") };
        let root_sub = Subscription::new(root_sub_meta)?;
        subs.update(&root_sub);
        Ok(
            PubSubServer {
                subs: subs,
                sessions: HashMap::new(),
        })
    }

    fn publish(&self, sub_id: &Uuid, p: &Publication) -> Result<(), ClientError> {
        match self.subs.fetch(sub_id) {
            Ok(sub) => {
                let res = sub.subscribers.iter().map(|s| {
                    if let Some(recipient) = self.sessions.get(&s.id()) {
                        recipient.do_send(p.clone()).unwrap();
                    } 
                });
                Ok(res.collect())
            },
            Err(e) => {
                Err(ClientError::InvalidInput(format!("{}", e)))
            }
        }
    }
}

impl Actor for PubSubServer {
    type Context = Context<Self>;
}

impl Handler<ClientMessage> for PubSubServer {
    type Result = Result<(), ClientError>;

    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) -> Result<(), ClientError> {
        match msg.req {
            ClientRequest::Add { param }=> {
                if let s = self.subs.fetch(&param)? {
                    Ok(s.handle_subscribers(&msg.id, 0))
                } else {
                    let new_sub_meta = SubscriptionMeta { name: format!("{}", msg.id) };
                    let new_sub = Subscription::new(new_sub_meta)?;
                    Ok(self.subs.update(&new_sub))
                }
            },
            ClientRequest::Get { param } => {
                if let s = self.subs.fetch(&param)? {
                    let subscription_info = Publication::new(&serde_json::to_string_pretty(&s)?);
                    self.sessions.get(&msg.id.id())
                                    .ok_or(ClientError::InvalidInput(String::from("Invalid ClientID")))?
                                    .do_send(subscription_info)
                                    .map_err(|e| ClientError::PublishingError(String::from("Failed sending requested Subscription")))
                } else {
                    Err(ClientError::InvalidInput(String::from("No such subscription")))
                }
            },
            ClientRequest::Publish { param } => {
                if let s = self.subs.fetch(&param.0)? {
                    Ok(self.publish(&s.id, &param.1)?)
                } else {
                    Err(ClientError::PublishingError(String::from("No such subscription")))
                }
            },
            ClientRequest::Remove { param } => {}
        }
    }
}