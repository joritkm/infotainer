use std::collections::HashSet;
use std::time::{Duration, Instant};

use actix::prelude::{Actor, ActorContext, Addr, AsyncContext, Handler, Running, StreamHandler};
use actix_web::{error, web};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    data_log::{DataLogEntry, DataLogFetch, DataLogIndex, DataLogger},
    prelude::ServerMessage,
    pubsub::{ManageSubscription, PubSubService, Publication, SubmitCommand},
    sessions::{InsertSession, RemoveSession, SessionService},
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Represents errors caused by client interaction
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

///Perform ws-handshake and create the socket.
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

///Run websocket via actor, track alivenes of clients
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

    /// On start of actor begin monitoring heartbeat and create
    /// a session on the `PubSubServer`
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Starting WebSocketSession for {}", self.id);
        self.beat(ctx);
        if let Err(e) = self
            .sessions
            .try_send(InsertSession::new(&self.id, &ctx.address()))
        {
            error!("{}", e);
            ctx.stop()
        }
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        info!("Stopping WebSocketSession for {}", self.id);
        self.sessions.do_send(RemoveSession::from(&self.id));
        Running::Stop
    }
}

impl Handler<ServerMessage<Publication>> for WebSocketSession {
    type Result = Result<(), ClientError>;

    fn handle(
        &mut self,
        publication: ServerMessage<Publication>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        debug!("Received {:?} for {}", publication, self.id);
        Ok(ctx.binary(serde_cbor::to_vec(&publication)?))
    }
}

impl Handler<ServerMessage<HashSet<Uuid>>> for WebSocketSession {
    type Result = Result<(), ClientError>;

    fn handle(
        &mut self,
        msg: ServerMessage<HashSet<Uuid>>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(ctx.binary(serde_cbor::to_vec(&msg)?))
    }
}

impl Handler<ServerMessage<DataLogEntry>> for WebSocketSession {
    type Result = Result<(), ClientError>;

    fn handle(
        &mut self,
        msg: ServerMessage<DataLogEntry>,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        Ok(ctx.binary(serde_cbor::to_vec(&msg)?))
    }
}

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
                    Ok(ClientCommand::GetLogEntries {
                        client_id,
                        log_id,
                        entries,
                    }) => {
                        if let Err(e) = self
                            .datalog
                            .try_send(DataLogFetch::new(&client_id, &log_id, &entries))
                        {
                            error!("Error while requesting DataLogEntries");
                            ctx.binary(format!("{}", e));
                        }
                    }
                    Ok(ClientCommand::GetLogIndex { client_id, log_id }) => {
                        if let Err(e) = self
                            .datalog
                            .try_send(DataLogIndex::new(&client_id, &log_id))
                        {
                            error!("Error while requesting DataLogIndex");
                            ctx.binary(format!("{}", e));
                        }
                    }
                    Ok(ClientCommand::SubmitPublication {
                        client_id,
                        subscription_id,
                        submission,
                    }) => {
                        if let Err(e) = self.pubsub.try_send(SubmitCommand::new(
                            &client_id,
                            &subscription_id,
                            &submission,
                        )) {
                            error!("Error during publication: {}", e);
                            ctx.binary(format!("{}", e));
                        }
                    }
                    Ok(ClientCommand::Subscribe {
                        client_id,
                        subscription_id,
                    }) => {
                        if let Err(e) = self.pubsub.try_send(ManageSubscription::Add {
                            client_id,
                            subscription_id,
                        }) {
                            error!("Error while attempting to subscribe client to subscription");
                            ctx.binary(format!("{}", e))
                        }
                    }
                    Ok(ClientCommand::Unsubscribe {
                        client_id,
                        subscription_id,
                    }) => {
                        if let Err(e) = self.pubsub.try_send(ManageSubscription::Add {
                            client_id,
                            subscription_id,
                        }) {
                            error!("Error while attempting to subscribe client to subscription");
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

/// Represents a command from a connected client
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum ClientCommand {
    /// Retrieve a Subscriptions log index
    GetLogIndex { client_id: Uuid, log_id: Uuid },
    /// Fetch one or more entries from the datalog
    GetLogEntries {
        client_id: Uuid,
        log_id: Uuid,
        entries: Vec<Uuid>,
    },
    /// Add client to a Subscription, creating it it if doesn't exist
    Subscribe {
        client_id: Uuid,
        subscription_id: Uuid,
    },
    /// Remove client from a Subscription, deleting it, if client was last subscriber
    Unsubscribe {
        client_id: Uuid,
        subscription_id: Uuid,
    },
    /// Submit new data for publication
    SubmitPublication {
        client_id: Uuid,
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
        let data_log = DataLogger::new(&test_dir, &sessions.clone().recipient())
            .unwrap()
            .start();
        let pubsub_server = PubSubService::new(&data_log, &sessions.clone().recipient()).start();
        let session_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let test_submission_data =
            serde_cbor::to_vec(&String::from("Milton Beats <Giver of Beatings>")).unwrap();
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
            client_id: session_id.clone(),
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
        let sub_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<String>(&a[..]).unwrap()),
            _ => None,
        };
        assert_eq!(sub_response, None);
        let pub_message = ClientCommand::SubmitPublication {
            client_id: session_id.clone(),
            subscription_id: subscription_id,
            submission: test_submission_data.clone(),
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
        let mut publication_data = Vec::new();
        match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => {
                publication_data = serde_cbor::from_slice::<ServerMessage<Publication>>(&a[..]).unwrap().0.data;
            }
            _ => (),
        };
        assert_eq!(test_submission_data, publication_data);
        let log_message = ClientCommand::GetLogIndex {
            client_id: session_id.clone(),
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
                log_response = serde_cbor::from_slice::<ServerMessage<HashSet<Uuid>>>(&a[..])
                    .unwrap()
                    .0
                    .as_ref()
                    .to_owned();
            }
            _ => (),
        };
        assert!(!&log_response.is_empty());
        let entry_message = ClientCommand::GetLogEntries {
            client_id: session_id.clone(),
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
        let mut entry_response = Vec::new();
        match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => {
                match serde_cbor::from_slice::<ServerMessage<DataLogEntry>>(&a[..])
                    .unwrap()
                    .0
                    .as_ref()
                    .to_owned()
                {
                    DataLogEntry::Publications(a) => entry_response = a.clone(),
                    _ => (),
                }
            }
            _ => (),
        };
        assert!(!entry_response.is_empty());
        assert_eq!(entry_response[0].data, test_submission_data);
        let unsub_message = ClientCommand::Unsubscribe {
            client_id: session_id,
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
