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

use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_files as fs;
use actix_web::{error, http, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

#[macro_use]
use serde::{Serialize, Deserialize};
use uuid::Uuid;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

///Perform ws-handshake and create the socket.
async fn wsa(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let res = ws::start(WebSock::new(), &req, stream);
    res
}

///Create a new ClientID
async fn new_client() -> Result<HttpResponse, Error> {
    let cli = ClientID::new();
    let res: String = cli.to_string();
    Ok(HttpResponse::build(http::StatusCode::OK)
        .content_type("text/plain; charset=utf-8")
        .body(res))
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

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum ParseError {
    #[fail(display = "Unknown request Parameter: {}", _0)]
    UnknownParameter(String),

    #[fail(display = "Unknown request Type: {}", _0)]
    UnknownType(String),

    #[fail(display = "Request parameter missing: {}", _0)]
    MissingParameter(String),

    #[fail(display = "Unable to parse provided input data: {}", _0)]
    InvalidInput(String),
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> ParseError {
        ParseError::InvalidInput(format!("{}", e))
    }
}

#[derive(Debug, PartialEq, Clone)]
enum ClientRequest {
    Get { param: String },
    Add { param: String },
    Remove { param: String },
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
            .ok_or(ParseError::MissingParameter(String::from(
                "Request Argument",
            )))
            .map_err(|e| error::ErrorBadRequest(e))?
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(256)
            .collect();
        match req_type.as_str() {
            "get" => Ok(ClientRequest::Get { param: req_arg }),
            "add" => Ok(ClientRequest::Add { param: req_arg }),
            "remove" => Ok(ClientRequest::Remove { param: req_arg }),
            _ => Err(ParseError::UnknownType(String::from(raw)))
                .map_err(|e| error::ErrorBadRequest(e)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
struct ClientID {
    uid: Uuid,
}

impl ClientID {
    fn new() -> ClientID {
        let id = ClientID {
            uid: Uuid::new_v4(),
        };
        id
    }
}

impl TryFrom<&str> for ClientID {
    type Error = uuid::Error;

    fn try_from(id: &str) -> Result<ClientID, Self::Error> {
        let uid = Uuid::parse_str(&id)?;
        Ok(ClientID { uid: uid })
    }
}

impl fmt::Display for ClientID {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.uid.to_simple())
    }
}

#[derive(Debug, PartialEq, Clone)]
struct ClientMessage {
    id: ClientID,
    req: ClientRequest,
}

impl ClientMessage {
    fn new(raw: &str) -> Result<ClientMessage, Error> {
        let mut msg_token = raw.split("|");
        let msg_id = msg_token
            .next()
            .ok_or(error::ErrorForbidden("Missing identification."))?;
        let msg_req = msg_token
            .next()
            .ok_or(error::ErrorBadRequest("Missing Request."))?;
        let id = ClientID::try_from(msg_id).map_err(|e| error::ErrorForbidden(e))?;
        let request = ClientRequest::from_str(msg_req)?;
        Ok(ClientMessage {
            id: id,
            req: request,
        })
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
struct SubscriptionMeta {
    name: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
struct Subscription {
    id: Uuid,
    metadata: Vec<u8>,
}

impl Subscription {
    fn new(raw: SubscriptionMeta) -> Result<Subscription, ParseError> {
        let meta = serde_json::to_vec(&raw)?;
        Ok(Subscription {
            id: Uuid::new_v4(),
            metadata: meta,
        })
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
struct Subscriptions {
    store: HashMap<Uuid, Box<Subscription>>,
}

impl Subscriptions {
    fn new() -> Subscriptions {
        Subscriptions {
            store: HashMap::new(),
        }
    }

    fn update(mut self, sub: Subscription) {
        self.store.insert(sub.id, Box::new(sub));
    }

    fn fetch(&self, id: &Uuid) -> Result<&Box<Subscription>, Error> {
        self.store
            .get(&id)
            .ok_or(error::ErrorNotFound("No such entry"))
    }

    fn remove(mut self, id: &Uuid) {
        self.store.remove(&id);
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
struct Sessions {
    store: HashMap<Uuid, Vec<Uuid>>,
}

impl Sessions {
    fn new() -> Sessions {
        Sessions {
            store: HashMap::new(),
        }
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var(
        "RUST_LOG",
        "actix_server=info,actix_web=info,infotainer=debug",
    );
    env_logger::init();
    let sessions = web::Data::new(Mutex::new(Sessions::new()));
    let subscriptions = web::Data::new(Mutex::new(Subscriptions::new()));

    HttpServer::new(move || {
        App::new()
            .app_data(sessions.clone())
            .app_data(subscriptions.clone())
            .wrap(middleware::Logger::default())
            .service(web::resource("/ws/").route(web::get().to(wsa)))
            .service(web::resource("/id").route(web::get().to(new_client)))
            .service(fs::Files::new("/", "static/").index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
