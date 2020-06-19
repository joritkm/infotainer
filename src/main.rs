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

use std::convert::TryFrom;
use actix_web::{ http, web, error, guard, middleware, HttpServer, App };
use actix_web_actors::ws;
use actix_files as fs;

use tokio::sync::Mutex;

use websocket::WebSock;
use session::Sessions;
use subscription::Subscriptions;
use client::ClientID;

mod errors;
mod websocket;
mod session;
mod subscription;
mod client;

// this brave static string guards the session registry
const MAGICTOKEN: &str = "magictoken";

///Perform ws-handshake and create the socket.
async fn wsa(req: web::HttpRequest, stream: web::Payload) -> Result<web::HttpResponse, error::Error> {
    let res = ws::start(WebSock::new(), &req, stream);
    res
}

///Create a new ClientID
async fn client_id(sessions: web::Data<Mutex<Sessions>>) -> Result<web::HttpResponse, error::Error> {
    // TODO: Pending the renaming of sessions, remove this and accept posts with their own uuids from new clients. 
    let client_id = ClientID::try_from("89e93972-cb91-4511-944d-30de98a87199").unwrap();
    let client = sessions.try_lock().map_err(|e| error::ErrorInternalServerError(e))?.get_or_insert(&client_id);
    Ok(web::HttpResponse::build(http::StatusCode::OK)
        .content_type("application/json; charset=utf-8")
        .json(client))
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
            .app_data(sessions.to_owned())
            .app_data(subscriptions.to_owned())
            .wrap(middleware::Logger::default())
            .service(web::resource("/ws/").route(web::get().to(wsa)))
            .service(
                web::resource("/id").route(
                    web::post()
                        .guard(guard::Header("X-Auth-Token", MAGICTOKEN))
                        .to(client_id),
                ),
            )
            .service(fs::Files::new("/", "static/").index_file("index.html"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}