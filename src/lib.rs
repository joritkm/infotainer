/*

MIT License

Copyright (c) 2020 joppich

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

#[macro_use]
extern crate log;

#[macro_use]
extern crate failure;

mod data_log;
mod pubsub;
mod sessions;
mod websocket;

pub mod prelude {
    pub use super::ServerMessage;
    pub use crate::data_log::DataLogger;
    pub use crate::pubsub::PubSubService;
    pub use crate::sessions::SessionService;
    pub use crate::websocket::websocket_handler;
}
use std::collections::HashSet;

use actix::prelude::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use data_log::DataLogEntry;
use pubsub::Publication;
use websocket::ClientError;

/// Represents a message sent by the server to a connected client
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ServerMessage<T>(Box<T>)
where
    T: Serialize;

impl From<&Publication> for ServerMessage<Publication> {
    fn from(publication: &Publication) -> ServerMessage<Publication> {
        ServerMessage(Box::new(publication.clone()))
    }
}

impl From<DataLogEntry> for ServerMessage<DataLogEntry> {
    fn from(entry: DataLogEntry) -> ServerMessage<DataLogEntry> {
        ServerMessage(Box::new(entry))
    }
}

impl From<&HashSet<Uuid>> for ServerMessage<HashSet<Uuid>> {
    fn from(hashset: &HashSet<Uuid>) -> ServerMessage<HashSet<Uuid>> {
        ServerMessage(Box::new(hashset.clone()))
    }
}
