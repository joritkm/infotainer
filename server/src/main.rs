extern crate actix;

extern crate actix_web;

extern crate env_logger;


use std::time::{Instant, Duration};

use actix::prelude::*;
use actix_web::{
    fs, http, middleware, server, ws, App, Error, HttpRequest, HttpResponse,
};


const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);


///Run websocket via actor, track alivenes of clients
struct WsSession {
    hb: Instant,
}

impl Actor for WsSession {
    type Context = ws:WebsocketContext<Self>;

    ///On start of actor begin monitoring heartbeat
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
    }
}

///Perform ws-handshake and create the socket.
fn wsa(r: &HttpRequest) -> Result(<HttpResponse,Error>) {
    ws::start(r, InfoWs::new())
}

