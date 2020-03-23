extern crate actix;

extern crate actix_web;

#[macro_use]
extern crate log;

extern crate env_logger;

use std::time::{Instant, Duration};
use std::str;

use actix::prelude::*;
use actix_web::{
    middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer,
};
use actix_web_actors::ws;
use actix_files as fs;


const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

///Perform ws-handshake and create the socket.
async fn wsa(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let res = ws::start(WebSock::new(), &req, stream);
    res
}

///Run websocket via actor, track alivenes of clients
struct WebSock {
    hb: Instant,
}

impl Actor for WebSock {
    type Context = ws::WebsocketContext<Self>;

    ///On start of actor begin monitoring heartbeat
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("New connection established.");
        self.beat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSock {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(ws::Message::Binary(msg)) => {
                self.hb = Instant::now();
                info!("Client message: {}", str::from_utf8(&msg).unwrap())
            }
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Close(_)) => {
                info!("Received CLOSE from client.");
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl WebSock {
    
    fn new() -> Self {
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

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_server=info,actix_web=info,infotainer=debug");
    env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .service(web::resource("/ws/").route(web::get().to(wsa)))
            .service(fs::Files::new("/", "static/").index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
