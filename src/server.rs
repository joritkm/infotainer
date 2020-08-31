use actix::prelude::Actor;
use actix_files as fs;
use actix_web::{middleware, web, App, HttpServer};

use infotainer::pubsub::PubSubServer;
use infotainer::websocket::wsa;

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
