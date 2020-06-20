use std::collections::HashMap;

use serde::{Serialize,Deserialize};
use uuid::Uuid;

use crate::client::ClientID;

/// Holds a clients subscription state
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ClientSession {
    /// A `crate::client::ClientID`.
    client: ClientID,
    /// Holds IDs of `crate::scubscription::Subscription`.
    subscriptions: Vec<uuid::Uuid>
}

impl From<(&Uuid,&Vec<Uuid>)> for ClientSession {
    /// Convert from entries in `crate::session::ClientSessions` 
    fn from(entry: (&Uuid,&Vec<Uuid>)) -> ClientSession {
        ClientSession {
            client: ClientID::from(entry.0.to_owned()),
            subscriptions: entry.1.to_owned()
        }
    }
}

/// Holds a `HashMap` of `ClientSession` data
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ClientSessions {
    store: HashMap<Uuid, Vec<Uuid>>,
}

impl ClientSessions {
    /// Initializes a new ClientSession store
    pub fn new() -> ClientSessions {
        ClientSessions {
            store: HashMap::new(),
        }
    }

    /// Tries to retrieve an entry from the ClientSession store. If it doesn't
    /// exist, it will create a new entry from the provided `crate::client::ClientID`.
    pub fn get_or_insert(&mut self, cli: &ClientID) -> ClientSession {
        if let Some(entry) = self.store.get_key_value(&cli.id()) {
            ClientSession::from(entry)
        } else {
            self.store.insert(cli.id().to_owned(), Vec::new());
            self.get_or_insert(cli)
        }
    }

    /// Updates a specific entry in the ClientSession store
    // TODO: make this fail if entry doesn't exist.
    pub fn update(&mut self, session: &ClientSession) {
        self.store.insert(session.client.id().to_owned(), session.subscriptions.to_owned());
    }

    /// Remove an entry from the Session store
    pub fn remove(&mut self, cli: &ClientID) {
        self.store.remove_entry(&cli.id());
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_session() {
        let dummy = ClientSession{ client: ClientID::from(Uuid::new_v4()),
                             subscriptions: vec![Uuid::new_v4(),Uuid::new_v4()] };
        let session_entry = (&dummy.client.id(), &dummy.subscriptions);
        let session: ClientSession = session_entry.into();
        assert_eq!(session, dummy);
    }

    #[test]
    fn test_sessions() {
        let subscription = Uuid::new_v4();
        let mut sessions = ClientSessions::new();
        let mut session = sessions.get_or_insert(&ClientID::from(Uuid::new_v4()));
        let session_entry = ClientSession{ client: session.client.to_owned(), subscriptions: vec![]};
        assert_eq!(session, session_entry);
        session.subscriptions.push(subscription);
        sessions.update(&session);
        let updated_entry = sessions.get_or_insert(&session.client);
        assert_eq!(session, updated_entry);
        sessions.remove(&session.client);
        assert_eq!(sessions.store, HashMap::new())
    }
}