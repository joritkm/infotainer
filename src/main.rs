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
 implement subscribe to subscription
 */

#[macro_use]
extern crate log;

#[macro_use]
extern crate failure;

extern crate serde_json;

mod errors;
mod protocol;
mod pubsub;
mod subscription;
mod websocket;

use actix::prelude::{Actor, Addr};
use actix_files as fs;
use actix_web::{error, middleware, web, App, HttpServer};
use actix_web_actors::ws;

use pubsub::PubSubServer;
use websocket::WebSocketSession;

///Perform ws-handshake and create the socket.
async fn wsa(
    req: web::HttpRequest,
    stream: web::Payload,
    pubsub_server: web::Data<Addr<PubSubServer>>,
) -> Result<web::HttpResponse, error::Error> {
    let websocket_session = WebSocketSession::new(pubsub_server.get_ref());
    ws::start(websocket_session, &req, stream)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let pubsub_server = PubSubServer::new().expect("Could not initiate PubSub server.");
    let addr = pubsub_server.start();
    HttpServer::new(move || {
        App::new()
            .data(addr.clone())
            .wrap(middleware::Logger::default())
            .service(web::resource("/ws/").route(web::get().to(wsa)))
            .service(fs::Files::new("/", "static/").index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
