use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::fs::{create_dir, create_dir_all, OpenOptions};
use std::path::{Path, PathBuf};

use actix::prelude::{
    Actor, ActorFuture, Addr, Context, Handler, Message, Recipient, ResponseActFuture,
    ResponseFuture, SendError, WrapFuture,
};
use faccess::{AccessMode, PathExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    prelude::ServerMessage, pubsub::Publication, sessions::GetSessionAddr,
    websocket::WebSocketSession,
};

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum DataLogError {
    #[fail(display = "Encountered error during file system interaction: {}", _0)]
    FileSystem(String),

    #[fail(display = "Failed during message handling: {}", _0)]
    DataLogRequest(String),

    #[fail(display = "Could not serialize data for writing to disk: {}", _0)]
    SerializerError(String),
}

impl From<SendError<DataLogPut>> for DataLogError {
    fn from(e: SendError<DataLogPut>) -> DataLogError {
        DataLogError::DataLogRequest(format!("{}", e))
    }
}

impl From<std::io::Error> for DataLogError {
    fn from(e: std::io::Error) -> DataLogError {
        DataLogError::FileSystem(format!("{}", e))
    }
}

impl From<serde_cbor::Error> for DataLogError {
    fn from(e: serde_cbor::Error) -> DataLogError {
        DataLogError::SerializerError(format!("{}", e))
    }
}

/// Represents a request for writing contained data
#[derive(Debug, Message)]
#[rtype("Result<(), DataLogError>")]
pub struct DataLogPut {
    data_log_id: Uuid,
    data_log_entry: DataLogEntry,
}

impl DataLogPut {
    /// Creates a new DataLogPut request from a Uuid and a DataLogEntry
    pub fn new(log_id: &Uuid, entry: DataLogEntry) -> DataLogPut {
        DataLogPut {
            data_log_id: *log_id,
            data_log_entry: entry,
        }
    }
}

/// A message to request a range of entries from a log collection
#[derive(Debug, Message)]
#[rtype("Result<(), DataLogError>")]
pub struct DataLogFetch {
    client_id: Uuid,
    data_log_id: Uuid,
    requested_datalog_entries: Vec<Uuid>,
}

impl DataLogFetch {
    /// Creates a new DataLogFetch request from a Uuid and an optional array of Uuids.
    pub fn new(client_id: &Uuid, subscription_id: &Uuid, entries: &Vec<Uuid>) -> Self {
        DataLogFetch {
            client_id: *client_id,
            data_log_id: *subscription_id,
            requested_datalog_entries: entries.clone(),
        }
    }
}

/// A message to request the data log index of a collection
#[derive(Debug, Message)]
#[rtype("Result<(), DataLogError>")]
pub struct DataLogIndex {
    client_id: Uuid,
    data_log_id: Uuid,
}

impl DataLogIndex {
    pub fn new(client_id: &Uuid, data_log_id: &Uuid) -> Self {
        DataLogIndex {
            client_id: client_id.clone(),
            data_log_id: data_log_id.clone(),
        }
    }
}

/// The Actor responsible for executing DataLog requests sent by
/// PubSubServer actors.
#[derive(Debug, Clone)]
pub struct DataLogger {
    sessions: Recipient<GetSessionAddr>,
    log_index: HashMap<Uuid, HashSet<Uuid>>,
    data_dir: PathBuf,
}

impl DataLogger {
    ///Creates a new DataLogger actor
    ///## Arguments
    ///* `data_dir_path` - The path to this DataLoggers data directory. Must exist and be accessible with rwx permissions.
    pub fn new(
        data_dir_path: &Path,
        sessions: &Recipient<GetSessionAddr>,
    ) -> Result<DataLogger, DataLogError> {
        if data_dir_path
            .access(AccessMode::EXISTS | AccessMode::READ | AccessMode::WRITE | AccessMode::EXECUTE)
            .is_ok()
        {
            create_dir_all(data_dir_path)?;
            Ok(DataLogger {
                log_index: HashMap::new(),
                sessions: sessions.clone(),
                data_dir: PathBuf::from(data_dir_path),
            })
        } else {
            Err(DataLogError::FileSystem(format!(
                "Could not access data directory with required permissions"
            )))
        }
    }
}

impl Actor for DataLogger {
    type Context = Context<DataLogger>;
}

impl Handler<DataLogPut> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, request: DataLogPut, _: &mut Context<Self>) -> Self::Result {
        let mut log_path = self.data_dir.join(&request.data_log_id.to_string());
        Ok(match &request.data_log_entry {
            DataLogEntry::Subscribers(subscribers) => {
                create_dir(&log_path)?;
                log_path.push("subscribers");
                let subscribers_file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&log_path)?;
                serde_cbor::to_writer(subscribers_file, &subscribers)?;
            }
            DataLogEntry::Publications(entries) => {
                log_path.push("publications");
                create_dir_all(&log_path)?;
                for item in entries {
                    &log_path.push(item.publication_id.to_string());
                    let entry_file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(&log_path)?;
                    serde_cbor::to_writer(entry_file, &item)?;
                    let log_index_entry = self
                        .log_index
                        .entry(request.data_log_id)
                        .or_insert(HashSet::new());
                    log_index_entry.insert(item.publication_id);
                    &log_path.pop();
                }
            }
        })
    }
}

impl Handler<DataLogIndex> for DataLogger {
    type Result = ResponseActFuture<Self, Result<(), DataLogError>>;

    fn handle(&mut self, request: DataLogIndex, _: &mut Context<Self>) -> Self::Result {
        let session_service = self.sessions.clone();
        let client = request.client_id;
        let log_id = request.data_log_id;
        Box::pin(
            async move {
                if let Ok(session_request) =
                    session_service.send(GetSessionAddr::from(&client)).await
                {
                    if let Ok(recipient) = session_request {
                        Some(recipient)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            .into_actor(self)
            .map(move |res, act, _| {
                if let Some(recipient) = res {
                    if let Some(log_index) = act.log_index.get(&log_id) {
                        match recipient.try_send(ServerMessage::from(log_index)) {
                            Ok(send_result) => Ok(send_result),
                            Err(e) => Err(DataLogError::DataLogRequest(format!("{}", e))),
                        }
                    } else {
                        Err(DataLogError::DataLogRequest(format!("No such data log")))
                    }
                } else {
                    Err(DataLogError::DataLogRequest(format!(
                        "Could not retrieve retrieve recipient from session service."
                    )))
                }
            }),
        )
    }
}

impl Handler<DataLogFetch> for DataLogger {
    type Result = ResponseFuture<Result<(), DataLogError>>;

    fn handle(&mut self, request: DataLogFetch, _: &mut Context<Self>) -> Self::Result {
        let mut log_path = self.data_dir.join(&request.data_log_id.to_string());
        let session_service = self.sessions.clone();
        let mut client_addr: Option<Addr<WebSocketSession>> = None;
        Box::pin(async move {
            if let Ok(session_request) = session_service
                .send(GetSessionAddr::from(&request.client_id))
                .await
            {
                if let Ok(recipient) = session_request {
                    client_addr = Some(recipient);
                }
            }
            &log_path.push("publications");
            let mut read_results = Vec::new();
            for item in request.requested_datalog_entries {
                &log_path.push(item.to_string());
                let entry_file = OpenOptions::new().read(true).open(&log_path)?;
                read_results.push(serde_cbor::from_reader(&entry_file)?);
                &log_path.pop();
            }
            if let Some(receiver) = client_addr {
                if let Err(e) = receiver.try_send(ServerMessage::from(DataLogEntry::Publications(
                    read_results,
                ))) {
                    Err(DataLogError::DataLogRequest(format!("{}", e)))
                } else {
                    Ok(())
                }
            } else {
                Err(DataLogError::DataLogRequest(format!(
                    "Could not retrieve recipient from session service"
                )))
            }
        })
    }
}

/// Holds data intended for writing. The variants indicate whether data should be held by a single file or by a file within a collection
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum DataLogEntry {
    Publications(Vec<Publication>),
    Subscribers(Vec<Uuid>),
}

impl From<&Publication> for DataLogEntry {
    fn from(p: &Publication) -> DataLogEntry {
        DataLogEntry::Publications(vec![p.clone()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    use crate::prelude::SessionService;

    fn create_test_directory() -> PathBuf {
        let mut p = temp_dir();
        p.push(format!("infotainer-{}", Uuid::new_v4().to_hyphenated()));
        std::fs::create_dir(&p).unwrap();
        p
    }

    fn remove_test_directory(p: &Path) {
        std::fs::remove_dir_all(p).unwrap();
    }

    #[actix_rt::test]
    async fn test_starting_data_logger() {
        let test_data_dir = create_test_directory();
        let sessions = SessionService::new().start();
        let data_logger = DataLogger::new(&test_data_dir, &sessions.clone().recipient()).unwrap();
        let data_logger_actor = data_logger.clone().start();

        assert_eq!(data_logger.data_dir, PathBuf::from(&test_data_dir));
        assert!(data_logger_actor.connected());
        remove_test_directory(&test_data_dir);
    }

    #[actix_rt::test]
    async fn test_starting_data_logger_failure() {
        let test_data_dir = Path::new("/frank/nord");
        let sessions = SessionService::new().start();
        let data_logger = DataLogger::new(test_data_dir, &sessions.clone().recipient());
        assert!(data_logger.is_err());
    }
}
