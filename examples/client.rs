use core::time;
use std::{convert::TryFrom, io::Error, str::FromStr, thread};

use actix::{
    io::{SinkWrite, WriteHandler},
    Actor, ActorContext, Arbiter, AsyncContext, Context, Handler, Message as ActorMessage,
    StreamHandler, System,
};
use actix_codec::Framed;
use actix_web::{
    client::{Client, WsProtocolError},
    web::Bytes,
};
use awc::{
    ws::{Codec, Frame, Message},
    BoxedSocket,
};
use futures::{stream::SplitSink, StreamExt};
use infotainer::prelude::*;
use itertools::Itertools;
use uuid::Uuid;

static CLI_COMMANDS: &[&str] = &["PublishText", "Subscribe", "Unsubscribe"];

struct Connection(SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>);

impl Actor for Connection {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.hb(ctx)
    }

    fn stopped(&mut self, _: &mut Context<Self>) {
        System::current().stop();
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for Connection {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, _: &mut Context<Self>) {
        match msg {
            Ok(frame) => match frame {
                Frame::Binary(data) => {
                    if let Ok(iss) = serde_cbor::from_slice::<ServerMessage>(&data) {
                        match iss {
                            ServerMessage::Issue(i) => {
                                let cmd = ClientCommand::GetLogEntries {log_id: i.0, entries: vec![i.1] };
                                self.0.write(Message::Binary(Bytes::from(serde_cbor::to_vec(&cmd).unwrap())));
                            },
                            ServerMessage::LogEntry(e) => {
                                for p in e {
                                    let data: String = String::from_utf8(p.data).unwrap();
                                    println!("Received publication {} for Subscription {}:\n{}", p.publication_id, p.subscription_id, data)
                                }
                            },
                            ServerMessage::LogIndex(i) => println!("{:?}", i)
                        }
                    } else {
                        println!("Unable to handle received message");
                    }
                }
                _ => (),
            },
            Err(e) => println!("{:?}", e),
        }
    }

    fn started(&mut self, _: &mut Context<Self>) {
        println!("Connected");
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        println!("Disconnected");
        ctx.stop();
    }
}

impl WriteHandler<WsProtocolError> for Connection {}

impl Handler<CliCommand> for Connection {
    type Result = ();

    fn handle(&mut self, msg: CliCommand, _: &mut Self::Context) -> Self::Result {
        self.0.write(Message::Binary(Bytes::from(
            serde_cbor::to_vec(&ClientCommand::from(msg.into())).unwrap(),
        )));
    }
}

impl Connection {
    fn hb(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(time::Duration::new(5, 0), |act, _| {
            act.0.write(Message::Ping(Bytes::new()));
        });
    }
}

#[derive(Debug, ActorMessage)]
#[rtype("()")]
enum CliCommand {
    PublishText(Uuid, String),
    Subscribe(Uuid),
    Unsubscribe(Uuid),
}

impl TryFrom<String> for CliCommand {
    type Error = Box<dyn std::error::Error>;

    fn try_from(cmdline: String) -> Result<Self, Self::Error> {
        let mut cli_input = cmdline.split(" ");
        let cmd = cli_input.next().ok_or(Error::new(
            std::io::ErrorKind::InvalidInput,
            "Missing command",
        ))?;
        if !CLI_COMMANDS.contains(&cmd) {}
        let id = Uuid::from_str(cli_input.next().ok_or(Error::new(
            std::io::ErrorKind::InvalidInput,
            "Missing log id parameter",
        ))?)?;
        match cmd {
            "PublishText" => Ok(CliCommand::PublishText(id, cli_input.join(" "))),
            "Subscribe" => Ok(CliCommand::Subscribe(id)),
            "Unsubscribe" => Ok(CliCommand::Unsubscribe(id)),
            _ => Err(Box::new(Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid command",
            ))),
        }
    }
}

impl Into<ClientCommand> for CliCommand {
    fn into(self) -> ClientCommand {
        match self {
            CliCommand::PublishText(subscription_id, submission) => {
                ClientCommand::SubmitPublication {
                    subscription_id,
                    submission: submission.into(),
                }
            }
            CliCommand::Subscribe(subscription_id) => ClientCommand::Subscribe { subscription_id },
            CliCommand::Unsubscribe(subscription_id) => {
                ClientCommand::Unsubscribe { subscription_id }
            }
        }
    }
}

fn main() -> std::io::Result<()> {
    env_logger::init();
    let client_id = Uuid::new_v4();

    let sys = System::new("infotainer-client-example");

    Arbiter::spawn(async move {
        let (response, framed) = Client::default()
            .ws(format!("ws://127.0.0.1:1312/ws/{}", client_id))
            .connect()
            .await
            .unwrap();
        println!("Response: {:?}", response);
        let (sink, stream) = framed.split();
        let conn = Connection::create(|ctx| {
            Connection::add_stream(stream, ctx);
            Connection(SinkWrite::new(sink, ctx))
        });
        thread::spawn(move || loop {
            let mut cmd = String::default();
            if let Err(e) = std::io::stdin().read_line(&mut cmd) {
                println!("Could not read from commandline: {:?}", e);
                return;
            }
            match CliCommand::try_from(cmd.strip_suffix("\n").unwrap().to_owned()) {
                Ok(c) => conn.do_send(c),
                Err(e) => println!("Error: {:?}", e),
            }
        });
    });
    sys.run()
}
