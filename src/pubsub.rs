use actix::prelude::{Actor, Context, Recipient, Message, Handler};

use uuid::Uuid;
use std::collections::HashMap;

use crate::protocol::{ClientID, ClientMessage};
use crate::errors::ClientError;
use crate::subscription::{Subscription, Subscriptions, SubscriptionMeta};


#[derive(Debug, PartialEq, Clone, Message)]
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

    fn publish(&self, sub_id: &Uuid, p: Publication) -> Result<(), ClientError> {
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
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) {
        match msg.req {
            
        }
    }
}