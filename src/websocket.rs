use std::time::{Duration, Instant};

use actix::prelude::{Actor, ActorContext, Addr, AsyncContext, Handler, Running, StreamHandler};
use actix_web::{error, web};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::data_log::LogIndexPut;
use crate::pubsub::ManageSession;
use crate::ServerMessage;
use crate::{
    data_log::{DataLogError, DataLogPull, DataLogPut, DataLogger, LogIndexPull},
    pubsub::{
        Issue, ManageSubscription, PubSubService, Publication, PublicationError, SubmitCommand,
    },
    sessions::SessionService,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Represents errors caused during client interaction
#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum ClientError {
    #[fail(display = "Invalid Input: {}", _0)]
    InvalidInput(String),
}

impl From<serde_cbor::Error> for ClientError {
    fn from(e: serde_cbor::Error) -> ClientError {
        ClientError::InvalidInput(format!("{}", e))
    }
}

impl From<uuid::Error> for ClientError {
    fn from(e: uuid::Error) -> ClientError {
        ClientError::InvalidInput(format!("{}", e))
    }
}

/// Start a new WebSocketSession for the requesting client and start the actor.
pub async fn websocket_handler(
    req: web::HttpRequest,
    stream: web::Payload,
    session_id: web::Path<Uuid>,
    sessions: web::Data<Addr<SessionService>>,
    pubsub: web::Data<Addr<PubSubService>>,
    datalog: web::Data<Addr<DataLogger>>,
) -> Result<web::HttpResponse, error::Error> {
    let websocket_session = WebSocketSession::new(
        pubsub.get_ref(),
        sessions.get_ref(),
        datalog.get_ref(),
        &session_id,
    );
    ws::start(websocket_session, &req, stream)
}

/// The actor responsible handling client-server communication.
#[derive(Debug, Clone)]
pub struct WebSocketSession {
    id: Uuid,
    hb: Instant,
    sessions: Addr<SessionService>,
    pubsub: Addr<PubSubService>,
    datalog: Addr<DataLogger>,
}

impl WebSocketSession {
    fn new(
        pubsub: &Addr<PubSubService>,
        sessions: &Addr<SessionService>,
        datalog: &Addr<DataLogger>,
        client_id: &Uuid,
    ) -> WebSocketSession {
        WebSocketSession {
            id: *client_id,
            hb: Instant::now(),
            sessions: sessions.clone(),
            pubsub: pubsub.clone(),
            datalog: datalog.clone(),
        }
    }

    fn beat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                warn!("Connection for {} timed out. Closing.", act.id);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for WebSocketSession {
    type Context = ws::WebsocketContext<Self>;

    // On start of actor begin monitoring heartbeat and create
    // a session on the `PubSubServer`
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Starting WebSocketSession for {}", self.id);
        self.beat(ctx);
        if let Err(e) = self.pubsub.try_send(ManageSession::Add {
            client_id: self.id,
            addr: ctx.address(),
        }) {
            error!("{}", e);
            ctx.stop()
        }
    }

    // Unregister with SessionService when stopping the actor
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        info!("Stopping WebSocketSession for {}", self.id);
        self.pubsub
            .do_send(ManageSession::Remove { client_id: self.id });
        Running::Stop
    }
}

// Handles publication messages sent by the server
impl Handler<Issue> for WebSocketSession {
    type Result = Result<(), PublicationError>;

    fn handle(&mut self, msg: Issue, ctx: &mut Self::Context) -> Self::Result {
        debug!("Received {:?} for {}", msg, self.id);
        let msg = ServerMessage::Issue(msg);
        Ok(ctx.binary(
            serde_cbor::to_vec(&msg).map_err(|e| PublicationError::Publishing(e.to_string()))?,
        ))
    }
}

// Handles log indices sent by the server
impl Handler<LogIndexPut> for WebSocketSession {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: LogIndexPut, ctx: &mut Self::Context) -> Self::Result {
        let msg = ServerMessage::LogIndex(msg);
        Ok(ctx.binary(serde_cbor::to_vec(&msg).map_err(|e| DataLogError::WriteError(e))?))
    }
}

// Handles DataLogEntries sent by the server
impl Handler<DataLogPut<Publication>> for WebSocketSession {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: DataLogPut<Publication>, ctx: &mut Self::Context) -> Self::Result {
        let msg = ServerMessage::LogEntry(msg.0);
        Ok(ctx.binary(serde_cbor::to_vec(&msg).map_err(|e| DataLogError::PutDataLogEntry(e))?))
    }
}

// Handles incoming websocket messages sent by clients
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        trace!("Message received: {:#?}", &msg);
        match msg {
            Ok(ws::Message::Text(_)) => {
                self.hb = Instant::now();
                info!("Received Text Message from {}", self.id);
                ctx.text(format!("Text messages not implemented"))
            }
            Ok(ws::Message::Binary(msg)) => {
                self.hb = Instant::now();
                info!("Received Binary Message from {}", self.id);
                match serde_cbor::from_slice::<ClientCommand>(&msg) {
                    Ok(ClientCommand::GetLogEntries { log_id, entries }) => {
                        if let Err(e) = self.datalog.try_send(DataLogPull {
                            client: ctx.address().recipient(),
                            data_log_id: log_id,
                            selection: entries,
                        }) {
                            error!("Error while requesting DataLogEntries");
                            ctx.binary(format!("{}", e));
                        }
                    }
                    Ok(ClientCommand::GetLogIndex { log_id }) => {
                        if let Err(e) = self.datalog.try_send(LogIndexPull {
                            client: ctx.address().recipient(),
                            data_log_id: log_id,
                        }) {
                            error!("Error while requesting DataLogIndex");
                            ctx.binary(format!("{}", e));
                        }
                    }
                    Ok(ClientCommand::SubmitPublication {
                        subscription_id,
                        submission,
                    }) => {
                        if let Err(e) = self.pubsub.try_send(SubmitCommand::new(
                            &self.id,
                            &subscription_id,
                            &submission,
                        )) {
                            error!("Error during publication: {}", e);
                            ctx.binary(format!("{}", e));
                        }
                    }
                    Ok(ClientCommand::Subscribe { subscription_id }) => {
                        if let Err(e) = self.pubsub.try_send(ManageSubscription::Add {
                            client_id: self.id,
                            subscription_id,
                        }) {
                            error!("Error while attempting to subscribe client to subscription");
                            ctx.binary(format!("{}", e))
                        }
                    }
                    Ok(ClientCommand::Unsubscribe { subscription_id }) => {
                        if let Err(e) = self.pubsub.try_send(ManageSubscription::Remove {
                            client_id: self.id,
                            subscription_id,
                        }) {
                            error!(
                                "Error while attempting to unsubscribe client from subscription"
                            );
                            ctx.binary(format!("{}", e))
                        }
                    }
                    Err(e) => {
                        error!("{}", &e);
                        ctx.binary(format!("{}", &e))
                    }
                };
            }
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Close(reason)) => {
                info!("Received CLOSE from client.");
                ctx.close(reason);
                ctx.stop();
            }
            _ => {
                info!("Unable to handle message");
                ctx.stop()
            }
        }
    }
}

/// Represents a message from a client sent to the websocket.
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum ClientCommand {
    /// Retrieve a Subscriptions log index
    GetLogIndex { log_id: Uuid },
    /// Fetch one or more entries from the datalog
    GetLogEntries { log_id: Uuid, entries: Vec<Uuid> },
    /// Add client to a Subscription, creating it it if doesn't exist
    Subscribe { subscription_id: Uuid },
    /// Remove client from a Subscription, deleting it, if client was last subscriber
    Unsubscribe { subscription_id: Uuid },
    /// Submit new data for publication
    SubmitPublication {
        subscription_id: Uuid,
        submission: Vec<u8>,
    },
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::convert::TryInto;
    use std::env::temp_dir;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    use actix_web::{test, web, App};
    use futures_util::{sink::SinkExt, stream::StreamExt};

    use crate::data_log::DataLogger;

    fn create_test_directory() -> PathBuf {
        let mut p = temp_dir();
        p.push(format!("infotainer-{}", Uuid::new_v4().to_hyphenated()));
        std::fs::create_dir(&p).unwrap();
        p
    }

    fn remove_test_directory(p: &Path) {
        std::fs::remove_dir_all(p).unwrap();
    }

    #[actix_rt::test]
    async fn test_websocket_pubsub_datalog_integration() {
        let test_dir = create_test_directory();
        let sessions = SessionService::new().start();
        let data_log = DataLogger::new(&test_dir).unwrap().start();
        let pubsub_server = PubSubService::new(&data_log).start();
        let session_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let test_data_text = "Milton Beats <Giver of Beatings>";
        let mut srv = test::start(move || {
            App::new()
                .data(pubsub_server.clone())
                .data(data_log.clone())
                .data(sessions.clone())
                .route("/{session_id}", web::get().to(websocket_handler))
        });
        let mut conn = srv
            .ws_at(&format!("/{}", session_id))
            .await
            .expect("Could not start ws connection");
        assert!(&conn.is_write_ready());
        let sub_message = ClientCommand::Subscribe {
            subscription_id: subscription_id,
        };
        &conn
            .send(ws::Message::Binary(
                serde_cbor::to_vec(&sub_message)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ))
            .await
            .unwrap();
        let pub_message = ClientCommand::SubmitPublication {
            subscription_id: subscription_id,
            submission: test_data_text.into(),
        };
        &conn
            .send(ws::Message::Binary(
                serde_cbor::to_vec(&pub_message)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ))
            .await
            .unwrap();
        let issue_server_message = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => serde_cbor::from_slice::<ServerMessage>(&a[..]).unwrap(),
            _ => panic!("Could not parse response"),
        };
        let published_issue = match issue_server_message {
            ServerMessage::Issue(i) => {
                assert_eq!(i.0, subscription_id);
                i
            }
            _ => panic!("Received unexpected response: {:?}", issue_server_message),
        };
        let log_message = ClientCommand::GetLogIndex {
            log_id: subscription_id,
        };
        &conn
            .send(ws::Message::Binary(
                serde_cbor::to_vec(&log_message)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ))
            .await
            .unwrap();
        let mut log_response = HashSet::new();
        match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => {
                match serde_cbor::from_slice::<ServerMessage>(&a[..]).unwrap() {
                    ServerMessage::LogIndex(i) => log_response = i.1,
                    _ => panic!("Received invalid response from server"),
                }
            }
            _ => (),
        };
        assert!(!&log_response.is_empty());
        assert!(&log_response.contains(&published_issue.1));
        let entry_message = ClientCommand::GetLogEntries {
            log_id: subscription_id,
            entries: log_response.drain().collect(),
        };
        &conn
            .send(ws::Message::Binary(
                serde_cbor::to_vec(&entry_message)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ))
            .await
            .unwrap();
        let entry_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => serde_cbor::from_slice::<ServerMessage>(&a[..]).unwrap(),
            _ => panic!("Received invalid server response"),
        };
        let data_log_entry = match entry_response {
            ServerMessage::LogEntry(e) => e[0].clone(),
            _ => panic!("Unexpected server message"),
        };
        assert_eq!(
            String::from_utf8(data_log_entry.data).unwrap(),
            test_data_text
        );
        let unsub_message = ClientCommand::Unsubscribe {
            subscription_id: subscription_id,
        };
        &conn
            .send(ws::Message::Binary(
                serde_cbor::to_vec(&unsub_message)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ))
            .await
            .unwrap();
        let unsub_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<String>(&a[..]).unwrap()),
            _ => None,
        };
        assert_eq!(unsub_response, None);
        remove_test_directory(&test_dir);
    }

    #[test]
    fn test_client_error() {
        let err = ClientError::InvalidInput(String::from("Test"));
        let err_display = format!("{}", err);
        assert_eq!("Invalid Input: Test", &err_display);
    }

    #[test]
    fn test_wrapping_cbor_errors() {
        if let Err(e) = serde_cbor::from_slice::<String>(&[23]) {
            let err = ClientError::from(e);
            assert_eq!(
                "Invalid Input: invalid type: integer `23`, expected a string",
                format!("{}", err)
            )
        }
    }

    #[test]
    fn test_wrapping_uuid_errors() {
        if let Err(e) = Uuid::from_str("notauuidstring") {
            let err = ClientError::from(e);
            assert_eq!(
                "Invalid Input: invalid length: expected one of [36, 32], found 14",
                format!("{}", err)
            )
        }
    }
}
