use std::collections::HashMap;

use actix::prelude::{Actor, Addr, Context, Handler, Message};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::websocket::WebSocketSession;

/// Represents errors caused during interaction with SessionService
#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum SessionError {
    #[fail(display = "Invalid Input: {}", _0)]
    SessionNotFound(String),
}

#[derive(Debug, Message)]
#[rtype("()")]
pub struct InsertSession {
    id: Uuid,
    addr: Addr<WebSocketSession>,
}

impl InsertSession {
    pub fn new(id: &Uuid, addr: &Addr<WebSocketSession>) -> Self {
        InsertSession {
            id: id.clone(),
            addr: addr.clone(),
        }
    }
}

#[derive(Debug, Message)]
#[rtype("()")]
pub struct RemoveSession {
    id: Uuid,
}

impl From<&Uuid> for RemoveSession {
    fn from(data: &Uuid) -> Self {
        RemoveSession { id: data.clone() }
    }
}

#[derive(Debug, Message)]
#[rtype("Result<Addr<WebSocketSession>, SessionError>")]
pub struct GetSessionAddr {
    id: Uuid,
}

impl From<&Uuid> for GetSessionAddr {
    fn from(data: &Uuid) -> Self {
        GetSessionAddr { id: data.clone() }
    }
}

///Stores currently active sessions
#[derive(Debug, PartialEq, Clone)]
pub struct SessionService {
    sessions: HashMap<Uuid, Addr<WebSocketSession>>,
}

impl Actor for SessionService {
    type Context = Context<Self>;
}

impl Handler<InsertSession> for SessionService {
    type Result = ();

    fn handle(&mut self, msg: InsertSession, _: &mut Context<Self>) -> Self::Result {
        self.insert_session(&msg.id, &msg.addr)
    }
}

impl Handler<RemoveSession> for SessionService {
    type Result = ();

    fn handle(&mut self, msg: RemoveSession, _: &mut Context<Self>) -> Self::Result {
        self.remove_session(&msg.id)
    }
}

impl Handler<GetSessionAddr> for SessionService {
    type Result = Result<Addr<WebSocketSession>, SessionError>;

    fn handle(&mut self, msg: GetSessionAddr, _: &mut Context<Self>) -> Self::Result {
        let res = self.get_session_addr(&msg.id)?;
        Ok(res.clone())
    }
}

impl SessionService {
    pub fn new() -> Self {
        SessionService {
            sessions: HashMap::new(),
        }
    }

    fn insert_session(&mut self, client_id: &Uuid, addr: &Addr<WebSocketSession>) {
        self.sessions.insert(client_id.clone(), addr.clone());
    }

    fn remove_session(&mut self, client_id: &Uuid) {
        self.sessions.remove(client_id);
    }

    fn get_session_addr(&self, client_id: &Uuid) -> Result<&Addr<WebSocketSession>, SessionError> {
        if let Some(entry) = self.sessions.get(client_id) {
            Ok(entry)
        } else {
            Err(SessionError::SessionNotFound(client_id.to_string()))
        }
    }
}
