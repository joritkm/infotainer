use std::convert::{TryFrom, TryInto};
use std::time::{Duration, Instant};

use actix::prelude::{
    Actor, ActorContext, Addr, AsyncContext, Handler, Message, Running, StreamHandler,
};
use actix_web::{error, web, web::Bytes};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pubsub::{PubSubServer, Publication, Response, ServerMessage, Subscription};

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
    pubsub_server: web::Data<Addr<PubSubServer>>,
) -> Result<web::HttpResponse, error::Error> {
    let websocket_session = WebSocketSession::new(pubsub_server.get_ref(), &session_id);
    ws::start(websocket_session, &req, stream)
}

///Run websocket via actor, track alivenes of clients
#[derive(Debug, PartialEq, Clone)]
pub struct WebSocketSession {
    id: Uuid,
    hb: Instant,
    subscriptions: Vec<Subscription>,
    broker: Addr<PubSubServer>,
}

impl WebSocketSession {
    fn new(pubsub_server: &Addr<PubSubServer>, client_id: &Uuid) -> WebSocketSession {
        WebSocketSession {
            id: *client_id,
            hb: Instant::now(),
            subscriptions: Vec::new(),
            broker: pubsub_server.clone(),
        }
    }

    fn beat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                warn!("Connection timed out. Closing.");
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
        let addr = ctx.address();
        let join = ClientJoin {
            id: self.id,
            addr: addr,
        };
        if let Err(_) = self.broker.try_send(join) {
            error!("WebSocketSession {} failed to join pubsub server.", self.id);
            ctx.text("Error: Could not connect to pubsub-server.");
            ctx.stop()
        }
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        info!("Stopping WebSocketSession for {}", self.id);
        self.broker.do_send(ClientDisconnect { id: self.id });
        Running::Stop
    }
}

impl Handler<ServerMessage<Response>> for WebSocketSession {
    type Result = Result<(), ClientError>;

    fn handle(
        &mut self,
        msg: ServerMessage<Response>,
        ctx: &mut Self::Context,
    ) -> Result<(), ClientError> {
        debug!("Received {:?} for {}", msg, self.id);
        Ok(ctx.binary(serde_cbor::to_vec(&msg)?))
    }
}

impl Handler<ServerMessage<Publication>> for WebSocketSession {
    type Result = Result<(), ClientError>;

    fn handle(
        &mut self,
        publication: ServerMessage<Publication>,
        ctx: &mut Self::Context,
    ) -> Result<(), ClientError> {
        debug!("Received {:?} for {}", publication, self.id);
        Ok(ctx.binary(serde_cbor::to_vec(&publication)?))
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(_)) => {
                self.hb = Instant::now();
                info!("Received Text Message from {}", self.id);
                ctx.text(format!("Text messages not implemented"))
            }
            Ok(ws::Message::Binary(msg)) => {
                self.hb = Instant::now();
                info!("Received Binary Message from {}", self.id);
                match ClientMessage::try_from(&msg) {
                    Ok(client_message) => {
                        trace!("Message received: {:#?}", &client_message);
                        self.broker.do_send(client_message)
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

/// Represents a request to add a new websocket session to the pubsub server
#[derive(Debug, PartialEq, Clone, Message)]
#[rtype("()")]
pub struct ClientJoin {
    pub id: Uuid,
    pub addr: Addr<WebSocketSession>,
}

/// Represents a request to remove a websocket session from the pubsub server
#[derive(Debug, PartialEq, Clone, Message)]
#[rtype("()")]
pub struct ClientDisconnect {
    pub id: Uuid,
}

/// Represents a message from a connected client,
/// including the clients identifying uuid and a request
#[derive(Debug, PartialEq, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<(), ClientError>")]
pub struct ClientMessage {
    pub id: Uuid,
    pub request: ClientRequest,
}

impl TryFrom<&Bytes> for ClientMessage {
    /// Attempts to create a ClientMessage from cbor encoded binary data received on
    /// the websocket.

    type Error = serde_cbor::Error;
    fn try_from(raw: &Bytes) -> Result<ClientMessage, serde_cbor::Error> {
        serde_cbor::from_slice::<ClientMessage>(&raw[..])
    }
}

impl TryInto<Bytes> for ClientMessage {
    // Attemtps to create actix_web::web::Bytes from cbor encoded ClientMessages

    type Error = serde_cbor::Error;
    fn try_into(self) -> Result<Bytes, serde_cbor::Error> {
        Ok(Bytes::from(serde_cbor::to_vec(&self)?))
    }
}

/// Represents a command from a connected client
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum ClientRequest {
    /// List all currently available subscriptions
    List,
    /// Fetch a Subscription's log index
    Get { param: Uuid },
    /// Add client to a Subscription, creating it, if it doesn't exist
    Add { param: Uuid },
    /// Remove client from a Subscription, deleting it, if the Subscription
    /// was created by client
    Remove { param: Uuid },
    /// Submit new data for publication to subscribed clients
    Submit { param: ClientSubmission },
}

/// Represents data intended for distribution to subscribers of Subscription `id`
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ClientSubmission {
    pub id: Uuid,
    pub data: Vec<u8>,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::convert::TryInto;
    use std::env::temp_dir;
    use std::iter::FromIterator;
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
    async fn test_pubsub_connection() {
        let pubsub_server = PubSubServer::default();
        let session_id = Uuid::new_v4();
        let addr = pubsub_server.start();
        let mut srv = test::start(move || {
            App::new()
                .data(addr.clone())
                .route("/{session_id}", web::get().to(websocket_handler))
        });
        let conn = srv.ws_at(&format!("/{}", session_id)).await.unwrap();
        assert!(&conn.is_write_ready());
    }

    #[actix_rt::test]
    async fn test_websocket_pubsub_datalog_integration() {
        let test_dir = create_test_directory();
        let data_log = DataLogger::new(&test_dir).unwrap().start();
        let pubsub_server =
            PubSubServer::new(Some(&data_log)).expect("Could not initiate PubSub server.");
        let session_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let test_submission_data =
            serde_cbor::to_vec(&String::from("Milton Beats <Giver of Beatings>")).unwrap();
        let add_message = ClientMessage {
            id: session_id.clone(),
            request: ClientRequest::Add {
                param: subscription_id,
            },
        };
        let list_message = ClientMessage {
            id: session_id,
            request: ClientRequest::List,
        };
        let pub_message = ClientMessage {
            id: session_id.clone(),
            request: ClientRequest::Submit {
                param: ClientSubmission {
                    id: subscription_id,
                    data: test_submission_data.clone(),
                },
            },
        };
        let get_message = ClientMessage {
            id: session_id.clone(),
            request: ClientRequest::Get {
                param: subscription_id,
            },
        };
        let remove_message = ClientMessage {
            id: session_id,
            request: ClientRequest::Remove {
                param: subscription_id,
            },
        };
        let addr = pubsub_server.start();
        let mut srv = test::start(move || {
            App::new()
                .data(addr.clone())
                .route("/{session_id}", web::get().to(websocket_handler))
        });
        let mut conn = srv.ws_at(&format!("/{}", session_id)).await.unwrap();
        assert!(&conn.is_write_ready());
        &conn
            .send(ws::Message::Binary(add_message.try_into().unwrap()))
            .await
            .unwrap();
        let add_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };
        &conn
            .send(ws::Message::Binary(list_message.try_into().unwrap()))
            .await
            .unwrap();
        let list_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };
        &conn
            .send(ws::Message::Binary(pub_message.try_into().unwrap()))
            .await
            .unwrap();
        let pub_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<Publication>(&a[..]).unwrap()),
            _ => None,
        };
        conn.next().await;
        &conn
            .send(ws::Message::Binary(get_message.try_into().unwrap()))
            .await
            .unwrap();
        let get_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };
        &conn
            .send(ws::Message::Binary(remove_message.try_into().unwrap()))
            .await
            .unwrap();
        let remove_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Binary(a) => Some(serde_cbor::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };

        assert_eq!(
            add_response.unwrap(),
            Response::Add {
                data: subscription_id
            }
        );
        assert_eq!(
            list_response.unwrap(),
            Response::List {
                data: vec![subscription_id]
            }
        );

        let pub_response_content = pub_response.unwrap();
        assert_eq!(pub_response_content.data, test_submission_data);
        assert_eq!(
            get_response.unwrap(),
            Response::Get {
                data: HashSet::from_iter(vec!(pub_response_content.id))
            }
        );
        assert_eq!(
            remove_response.unwrap(),
            Response::Remove {
                data: subscription_id
            }
        );
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
