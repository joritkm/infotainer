use std::time::{Duration, Instant};
use uuid::Uuid;

use actix::prelude::{Addr, AsyncContext, StreamHandler};
use actix::{Actor, ActorContext};
use actix_web_actors::ws;

use crate::protocol::{ClientID, ClientMessage};
use crate::pubsub::PubSubServer;
use crate::subscription::Subscription;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

///Run websocket via actor, track alivenes of clients
pub struct WebSocketSession {
    id: ClientID,
    hb: Instant,
    subscriptions: Vec<Subscription>,
    broker: Addr<PubSubServer>,
}

impl WebSocketSession {
    pub fn new(pubsub_server: &Addr<PubSubServer>) -> WebSocketSession {
        WebSocketSession {
            id: ClientID::from(Uuid::new_v4()),
            hb: Instant::now(),
            subscriptions: Vec::new(),
            broker: pubsub_server.clone(),
        }
    }
}

impl WebSocketSession {
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

    ///On start of actor begin monitoring heartbeat
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("New connection established.");
        self.beat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(msg)) => {
                self.hb = Instant::now();
                debug!("Message received: {:?}", &msg);
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
