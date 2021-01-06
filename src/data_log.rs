use std::fmt::Debug;
use std::fs::{create_dir, create_dir_all, OpenOptions};
use std::path::{Path, PathBuf};

use actix::prelude::{Actor, Context, Handler, Message, SendError};
use faccess::{AccessMode, PathExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pubsub::Publication;

#[derive(Debug, Fail, PartialEq, Clone, Serialize, Deserialize)]
pub enum DataLogError {
    #[fail(display = "Encountered error during file system interaction: {}", _0)]
    FileSystem(String),

    #[fail(display = "Sending DataLog message to DataLogger failed: {}", _0)]
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

/// Represents a response to data log entry fetch requests
#[derive(Debug, Message)]
#[rtype("Result<DataLogEntry, DataLogError>")]
pub struct DataLogFetch {
    data_log_id: Uuid,
    requested_datalog_entries: Option<Vec<Uuid>>,
}

impl DataLogFetch {
    /// Creates a new DataLogFetch request from a Uuid and an optional array of Uuids.
    pub fn new(subscription_id: &Uuid, entries: Option<Vec<Uuid>>) -> Self {
        DataLogFetch {
            data_log_id: *subscription_id,
            requested_datalog_entries: entries,
        }
    }
}

/// The Actor responsible for executing DataLog requests sent by
/// PubSubServer actors.
/// ## Example
/// ```
/// # use std::fs::{create_dir, remove_dir_all};
/// use std::path::Path;
/// use infotainer::prelude::DataLogger;
///
/// let data_dir = Path::new("/tmp/infotainer");
/// # create_dir(data_dir);
/// let data_logger = DataLogger::new(data_dir).unwrap();
/// # remove_dir_all(data_dir);
/// ```
#[derive(Debug, Clone)]
pub struct DataLogger {
    data_dir: PathBuf,
}

impl DataLogger {
    ///Creates a new DataLogger actor
    ///## Arguments
    ///* `data_dir_path` - The path to this DataLoggers data directory. Must exist and be accessible with rwx permissions.
    pub fn new(data_dir_path: &Path) -> Result<DataLogger, DataLogError> {
        if data_dir_path
            .access(AccessMode::EXISTS | AccessMode::READ | AccessMode::WRITE | AccessMode::EXECUTE)
            .is_ok()
        {
            create_dir_all(data_dir_path)?;
            Ok(DataLogger {
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
        create_dir(&log_path)?;
        Ok(match &request.data_log_entry {
            DataLogEntry::Subscribers(subscribers) => {
                log_path.push("subscribers");
                let subscribers_file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&log_path)?;
                serde_cbor::to_writer(subscribers_file, &subscribers)?;
            }
            DataLogEntry::Publications(entries) => {
                log_path.push("publications");
                create_dir(&log_path)?;
                for item in entries {
                    &log_path.push(item.id.to_string());
                    let entry_file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(&log_path)?;
                    serde_cbor::to_writer(entry_file, &item)?;
                    &log_path.pop();
                }
            }
        })
    }
}

impl Handler<DataLogFetch> for DataLogger {
    type Result = Result<DataLogEntry, DataLogError>;

    fn handle(&mut self, request: DataLogFetch, _: &mut Context<Self>) -> Self::Result {
        let mut log_path = self.data_dir.join(&request.data_log_id.to_string());
        let res = match request.requested_datalog_entries {
            Some(entries) => {
                log_path.push("publications");
                let mut read_results = Vec::new();
                for item in entries {
                    &log_path.push(item.to_string());
                    let entry_file = OpenOptions::new().read(true).open(&log_path)?;
                    read_results.push(serde_cbor::from_reader(&entry_file)?);
                }
                DataLogEntry::Publications(read_results)
            }
            _ => {
                log_path.push("subscribers");
                let subscribers_file = OpenOptions::new().read(true).open(&log_path)?;
                DataLogEntry::Subscribers(serde_cbor::from_reader(&subscribers_file)?)
            }
        };
        Ok(res)
    }
}

/// Holds data intended for writing. The variants indicate whether data should be held by a single file or by a file within a collection
#[derive(Debug, PartialEq)]
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

        let data_logger = DataLogger::new(&test_data_dir).unwrap();
        let data_logger_actor = data_logger.clone().start();

        assert_eq!(data_logger.data_dir, PathBuf::from(&test_data_dir));
        assert!(data_logger_actor.connected());
        remove_test_directory(&test_data_dir);
    }

    #[actix_rt::test]
    async fn test_starting_data_logger_failure() {
        let test_data_dir = Path::new("/frank/nord");
        let data_logger = DataLogger::new(test_data_dir);
        assert!(data_logger.is_err());
    }

    #[actix_rt::test]
    async fn test_data_log_item_entry() {
        let test_data_dir = create_test_directory();
        let dummy_data = (0..9).map(|_| Uuid::new_v4()).collect();
        let test_write_request = DataLogPut {
            data_log_id: Uuid::new_v4(),
            data_log_entry: DataLogEntry::Subscribers(dummy_data),
        };
        let test_read_request = DataLogFetch {
            data_log_id: test_write_request.data_log_id,
            requested_datalog_entries: None,
        };

        let data_logger = DataLogger::new(&test_data_dir).unwrap();
        let data_logger_actor = data_logger.start();

        let write_result = data_logger_actor.send(test_write_request).await;
        let read_result = data_logger_actor.send(test_read_request).await;
        assert!(write_result.is_ok());
        assert!(read_result.is_ok());
        remove_test_directory(&test_data_dir);
    }

    #[actix_rt::test]
    async fn test_data_log_collection_item_entry() {
        let test_data_dir = create_test_directory();
        let dummy_data = Publication {
            id: Uuid::new_v4(),
            data: "Test".as_bytes().to_owned(),
        };
        let test_write_request = DataLogPut {
            data_log_id: Uuid::new_v4(),
            data_log_entry: DataLogEntry::Publications(vec![dummy_data.clone()]),
        };
        let test_read_request = DataLogFetch {
            data_log_id: test_write_request.data_log_id,
            requested_datalog_entries: Some(vec![dummy_data.id]),
        };

        let data_logger = DataLogger::new(&test_data_dir).unwrap();
        let data_logger_actor = data_logger.start();

        let write_result = data_logger_actor.send(test_write_request).await;
        let read_result = data_logger_actor.send(test_read_request).await;
        assert!(write_result.unwrap().is_ok());
        assert!(read_result.unwrap().is_ok());
        remove_test_directory(&test_data_dir);
    }
}
