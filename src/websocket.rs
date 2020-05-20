use std::time::{Duration, Instant};

use actix::prelude::{AsyncContext, StreamHandler};
use actix::{Actor, ActorContext};
use actix_web_actors::ws;

use crate::client::ClientMessage;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

///Run websocket via actor, track alivenes of clients
pub struct WebSock {
    hb: Instant,
}

impl WebSock {
    pub fn new() -> Self {
        Self { hb: Instant::now() }
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

impl Actor for WebSock {
    type Context = ws::WebsocketContext<Self>;

    ///On start of actor begin monitoring heartbeat
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("New connection established.");
        self.beat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSock {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(msg)) => {
                self.hb = Instant::now();
                debug!("Message received: {:?}", ClientMessage::new(&msg));
            }
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Close(_)) => {
                debug!("Received CLOSE from client.");
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}