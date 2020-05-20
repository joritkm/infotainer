use std::collections::HashMap;

use serde::{Serialize,Deserialize};
use uuid::Uuid;

use crate::client::ClientID;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Session {
    client: ClientID,
    subscriptions: Vec<uuid::Uuid>
}

impl From<(&Uuid,&Vec<Uuid>)> for Session {
    fn from(entry: (&Uuid,&Vec<Uuid>)) -> Session {
        Session {
            client: ClientID::from(entry.0.to_owned()),
            subscriptions: entry.1.to_owned()
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Sessions {
    store: HashMap<Uuid, Vec<Uuid>>,
}

impl Sessions {
    pub fn new() -> Sessions {
        Sessions {
            store: HashMap::new(),
        }
    }

    pub fn get_or_insert(&mut self, cli: &ClientID) -> Session {
        if let Some(entry) = self.store.get_key_value(&cli.id()) {
            Session::from(entry)
        } else {
            self.store.insert(cli.id().to_owned(), Vec::new());
            self.get_or_insert(cli)
        }
    }

    pub fn update(&mut self, session: &Session) {
        if ! session.subscriptions.is_empty() {
            self.store.insert(session.client.id().to_owned(), session.subscriptions.to_owned());
        }
    }

    pub fn remove(&mut self, cli: &ClientID) {
        self.store.remove_entry(&cli.id());
    }
}
