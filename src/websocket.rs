use std::convert::TryFrom;
use std::time::{Duration, Instant};
use uuid::Uuid;

use actix::prelude::{Addr, AsyncContext, Handler, Running, StreamHandler};
use actix::{Actor, ActorContext};
use actix_web::{error, web};
use actix_web_actors::ws;

use crate::errors::ClientError;
use crate::protocol::{
    ClientDisconnect, ClientJoin, ClientMessage, Publication, Response, ServerMessage,
};
use crate::pubsub::PubSubServer;
use crate::subscription::Subscription;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

///Perform ws-handshake and create the socket.
pub async fn wsa(
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
    pub fn new(pubsub_server: &Addr<PubSubServer>, client_id: &Uuid) -> WebSocketSession {
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
        Ok(ctx.text(serde_json::to_string(&msg)?))
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
        Ok(ctx.text(serde_json::to_string(&publication)?))
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(msg)) => {
                self.hb = Instant::now();
                info!("Received Message from {}", self.id);
                match ClientMessage::try_from(msg.as_str()) {
                    Ok(client_message) => {
                        trace!("Message received: {:#?}", &client_message);
                        self.broker.do_send(client_message)
                    }
                    Err(e) => {
                        error!("{}", &e);
                        ctx.text(format!("Could not parse message {}", &msg))
                    }
                }
            }
            Ok(ws::Message::Binary(_msg)) => {
                self.hb = Instant::now();
                ctx.binary(format!("Unexpected binary data received"))
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::protocol::{ClientRequest, ClientSubmission};
    use actix_web::{test, web, App};
    use futures_util::{sink::SinkExt, stream::StreamExt};

    #[actix_rt::test]
    async fn test_websocket_pubsub_connection() {
        let pubsub_server = PubSubServer::new().expect("Could not initiate PubSub server.");
        let session_id = Uuid::new_v4();
        let addr = pubsub_server.start();
        let mut srv = test::start(move || {
            App::new()
                .data(addr.clone())
                .route("/{session_id}", web::get().to(wsa))
        });
        let conn = srv.ws_at(&format!("/{}", session_id)).await.unwrap();
        assert!(&conn.is_write_ready());
    }

    #[actix_rt::test]
    async fn test_websocket_messages() {
        let pubsub_server = PubSubServer::new().expect("Could not initiate PubSub server.");
        let session_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let test_submission_data = String::from("Milton Beats <Giver of Beatings>");
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
            request: ClientRequest::Publish {
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
                param: subscription_id
            }
        };
        let addr = pubsub_server.start();
        let mut srv = test::start(move || {
            App::new()
                .data(addr.clone())
                .route("/{session_id}", web::get().to(wsa))
        });
        let mut conn = srv.ws_at(&format!("/{}", session_id)).await.unwrap();
        assert!(&conn.is_write_ready());
        &conn.send(ws::Message::Text(
            serde_json::to_string(&add_message).unwrap(),
        ))
        .await
        .unwrap();
        let add_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Text(a) => Some(serde_json::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };
        &conn.send(ws::Message::Text(
            serde_json::to_string(&list_message).unwrap(),
        ))
        .await
        .unwrap();
        let list_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Text(a) => Some(serde_json::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };
        &conn.send(ws::Message::Text(
            serde_json::to_string(&pub_message).unwrap(),
        ))
        .await
        .unwrap();
        let pub_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Text(a) => Some(serde_json::from_slice::<Publication>(&a[..]).unwrap()),
            _ => None,
        };
        conn.next().await;
        &conn.send(ws::Message::Text(
            serde_json::to_string(&get_message).unwrap(),
        ))
        .await
        .unwrap();
        let get_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Text(a) => Some(serde_json::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };
        &conn.send(ws::Message::Text(
            serde_json::to_string(&remove_message).unwrap(),
        ))
        .await
        .unwrap();
        let remove_response = match conn.next().await.unwrap().unwrap() {
            ws::Frame::Text(a) => Some(serde_json::from_slice::<Response>(&a[..]).unwrap()),
            _ => None,
        };

        assert_eq!(serde_json::from_str::<Uuid>(&add_response.unwrap().data).unwrap(), subscription_id);
        assert_eq!(serde_json::from_str::<Vec<Uuid>>(&list_response.unwrap().data).unwrap(), vec![subscription_id]);
        assert_eq!(pub_response.unwrap().data, test_submission_data);
        assert_eq!(serde_json::from_str::<Vec<String>>(&get_response.unwrap().data).unwrap(), vec![test_submission_data]);
        assert_eq!(serde_json::from_str::<Uuid>(&remove_response.unwrap().data).unwrap(), subscription_id);
    }
}
