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

TODO: 
 implement client message parsing
 implement subscription table
 implement subscribe to subscription
 */

extern crate actix;

extern crate actix_web;

extern crate serde;

#[macro_use]
extern crate log;

#[macro_use]
extern crate failure;

extern crate env_logger;

use std::time::{Instant, Duration};

use actix::prelude::*;
use actix_web::{
    middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer, error
};
use actix_web_actors::ws;
use actix_files as fs;

use uuid::Uuid;

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
        debug!("New connection established.");
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

#[derive(Debug, Fail, PartialEq, Clone)]
pub enum ParseError {
    #[fail(display="Unknown request Parameter: {}", _0)]
    UnknownParameter(String),

    #[fail(display="Unknown request Type: {}", _0)]
    UnknownType(String),

    #[fail(display="Request parameter missing: {}", _0)]
    MissingParameter(String)
}

#[derive(Debug, PartialEq, Clone)]
enum ClientRequest {
    Get {param: String },
    Add { param: String },
    Remove { param: String }
}

impl ClientRequest {
    fn from_str(raw: &str) -> Result<ClientRequest, Error> {
        let mut params = raw.split("::");
        let req_type = params
            .next()
            .ok_or(ParseError::MissingParameter(String::from("Request type")))
            .map_err(|e| error::ErrorBadRequest(e))?
            .to_lowercase();
        let req_arg: String = params
            .next()
            .ok_or(ParseError::MissingParameter(String::from("Request Argument")))
            .map_err(|e| error::ErrorBadRequest(e))?
            .chars()
            .filter(|c| { c.is_alphanumeric() })
            .take(256)
            .collect();
        match req_type.as_str() {
            "get" => {
                Ok(ClientRequest::Get{ param : req_arg })
            },
            "add" => {
                Ok(ClientRequest::Add{ param: req_arg})
            },
            "remove" => {
                Ok(ClientRequest::Remove{ param: req_arg})
            },
            _ => {
                Err(ParseError::UnknownType(String::from(raw)))
                    .map_err(|e| error::ErrorBadRequest(e))
            }
        }       
    }
}

#[derive(Debug, PartialEq, Clone)]
struct ClientMessage {
    req: ClientRequest,
}

impl ClientMessage {
    fn new(raw: &str) -> Result<ClientMessage, Error> {
        let request = ClientRequest::from_str(raw)?;
        Ok(ClientMessage {req: request})   
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
