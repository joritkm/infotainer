use std::fmt::Debug;
use std::fs::{create_dir_all, read_dir, DirEntry, OpenOptions};
use std::path::{Path, PathBuf};
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use actix::prelude::{Actor, Context, Handler, Message, Recipient, SendError};
use thiserror::Error;
use faccess::{AccessMode, PathExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::pubsub::{Publication, Subscription};

pub type DataLogIndex = HashMap<Uuid, HashSet<Uuid>>;

#[derive(Debug, Error)]
pub enum DataLogError {
    #[error("Fs error: {0}")]
    FileSystem(String),

    #[error("Failed sending index: {0:?}")]
    PullIndex(#[source] SendError<LogIndexPut>),

    #[error("Failed sending log entries: {0:?}")]
    PullDataLogEntry(#[source] SendError<DataLogPut<Publication>>),

    #[error("Could not process DataLogPut: {0}")]
    PutDataLogEntry(#[source] serde_cbor::Error),

    #[error("Could not write data: {0:?}")]
    WriteError(#[source] serde_cbor::Error),

    #[error("Could not read data: {0:?}")]
    ReadError(#[source] serde_cbor::Error),
}

impl From<std::io::Error> for DataLogError {
    fn from(e: std::io::Error) -> DataLogError {
        DataLogError::FileSystem(format!("{}", e))
    }
}

/// A message to request a range of entries from a log collection
#[derive(Debug, Message)]
#[rtype("Result<(), DataLogError>")]
pub struct DataLogPull {
    pub data_log_id: Uuid,
    pub client: Recipient<DataLogPut<Publication>>,
    pub selection: Vec<Uuid>,
}

/// A message to request collection metadata
#[derive(Debug, Message)]
#[rtype("Result<(), DataLogError>")]
pub enum MetadataPull {
    Single(Uuid),
    All,
}

/// A message to request the data log index of a collection
#[derive(Debug, Message)]
#[rtype("Result<(), DataLogError>")]
pub struct LogIndexPull {
    pub client: Recipient<LogIndexPut>,
    pub data_log_id: Uuid,
}

/// Message type for one or more log entries
#[derive(Debug, Deserialize, PartialEq, Message, Serialize)]
#[rtype("Result<(), DataLogError>")]
pub struct DataLogPut<T: Serialize>(pub Vec<T>);

impl Into<Vec<Publication>> for DataLogPut<Publication> {
    fn into(self) -> Vec<Publication> {
        self.0
    }
}

/// Message type for Metadata of a collection
#[derive(Debug, PartialEq, Message)]
#[rtype("Result<(), DataLogError>")]
pub struct MetadataPut<T: Serialize + DeserializeOwned>(T);

/// Message Type for sending collection index
#[derive(Debug, Deserialize, PartialEq, Message, Serialize)]
#[rtype("Result<(), DataLogError>")]
pub struct LogIndexPut(Uuid, pub HashSet<Uuid>);

/// The Actor responsible for processing DataLog requests sent by
/// PubSubServer actors.
#[derive(Debug, Clone)]
pub struct DataLogger {
    log_index: DataLogIndex,
    data_dir: PathBuf,
}

impl DataLogger {
    ///Creates a new DataLogger actor
    ///## Arguments
    ///* `app_dir` - The application base directory. Must exist and be accessible with rwx permissions.
    pub fn new(app_dir: &Path) -> Result<DataLogger, DataLogError> {
        if app_dir
            .access(AccessMode::EXISTS | AccessMode::READ | AccessMode::WRITE | AccessMode::EXECUTE)
            .is_ok()
        {
            let data_dir_path = app_dir.join("data");
            create_dir_all(&data_dir_path)?;
            Ok(DataLogger {
                log_index: HashMap::new(),
                data_dir: PathBuf::from(&data_dir_path),
            })
        } else {
            Err(DataLogError::FileSystem(format!(
                "Could not access application base directory with required permissions"
            )))
        }
    }

    fn get_collection_log_path(&self, data_log_id: &Uuid) -> PathBuf {
        let mut path = self.data_dir.join(data_log_id.to_string());
        path.push("log");
        path
    }

    fn _list_entry_ids<P: AsRef<Path>, F: Fn(&DirEntry) -> bool>(
        &self,
        path: P,
        condition: F,
    ) -> Result<Vec<Uuid>, DataLogError> {
        let mut results = Vec::new();
        let entries = read_dir(path)?;
        for entry in entries {
            let dir_entry = entry?;
            if condition(&dir_entry) {
                if let Some(dir_name) = dir_entry.file_name().to_str() {
                    if let Some(collection_id) = Uuid::from_str(dir_name).ok() {
                        results.push(collection_id)
                    }
                }
            }
        }
        Ok(results)
    }

    fn read_data_file<T: Serialize + DeserializeOwned>(
        &self,
        filename: &str,
        path: &PathBuf,
    ) -> Result<T, DataLogError> {
        let file = OpenOptions::new().read(true).open(path.join(filename))?;
        serde_cbor::from_reader(&file).map_err(|e| DataLogError::ReadError(e))
    }

    fn write_data_file<T: Serialize>(
        &self,
        filename: &str,
        path: &PathBuf,
        data: T,
    ) -> Result<(), DataLogError> {
        create_dir_all(path)?;
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&path.join(filename))?;
        serde_cbor::to_writer(file, &data).map_err(|e| DataLogError::WriteError(e))
    }
}

impl Actor for DataLogger {
    type Context = Context<DataLogger>;
}

impl Handler<MetadataPull> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: MetadataPull, _: &mut Context<Self>) -> Self::Result {
        Ok(match &msg {
            MetadataPull::Single(subscription_id) => {
                let log_path = self.data_dir.join(subscription_id.to_string());
                self.read_data_file("metadata.cbor", &log_path)?;
            }
            MetadataPull::All => {}
        })
    }
}

impl Handler<MetadataPut<Subscription>> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: MetadataPut<Subscription>, _: &mut Context<Self>) -> Self::Result {
        let log_path = self.get_collection_log_path(&msg.0.id);
        self.write_data_file("metadata.cbor", &log_path, &msg.0)
    }
}

impl Handler<LogIndexPull> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: LogIndexPull, _: &mut Context<Self>) -> Self::Result {
        Ok(
            if let Some(log_index_entry) = self.log_index.get_key_value(&msg.data_log_id).clone() {
                &msg.client
                    .try_send(LogIndexPut(*log_index_entry.0, log_index_entry.1.clone()))
                    .map_err(|e| DataLogError::PullIndex(e))?;
            },
        )
    }
}

impl Handler<DataLogPull> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: DataLogPull, _: &mut Context<Self>) -> Self::Result {
        let log_path = self.get_collection_log_path(&msg.data_log_id);
        let mut read_results = Vec::new();
        for item in msg.selection {
            read_results.push(self.read_data_file(&item.to_string(), &log_path)?);
        }
        msg.client
            .try_send(DataLogPut(read_results))
            .map_err(|e| DataLogError::PullDataLogEntry(e))
    }
}

impl Handler<DataLogPut<Publication>> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: DataLogPut<Publication>, _: &mut Context<Self>) -> Self::Result {
        Ok(for item in msg.0 {
            let log_path = self.get_collection_log_path(&item.subscription_id);
            self.write_data_file(&item.publication_id.to_string(), &log_path, &item)?;
            let log_index_entry = self
                .log_index
                .entry(item.subscription_id)
                .or_insert(HashSet::new());
            log_index_entry.insert(item.publication_id);
        })
    }
}

impl Handler<LogIndexPut> for DataLogger {
    type Result = Result<(), DataLogError>;

    fn handle(&mut self, msg: LogIndexPut, _: &mut Context<Self>) -> Self::Result {
        self.log_index.insert(msg.0, msg.1);
        Ok(())
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
        let test_dir = create_test_directory();
        let data_logger = DataLogger::new(&test_dir).unwrap();
        let data_logger_actor = data_logger.clone().start();
        let mut test_data_dir = PathBuf::from(&test_dir);
        test_data_dir.push("data");
        assert_eq!(data_logger.data_dir, test_data_dir);
        assert!(data_logger_actor.connected());
        remove_test_directory(&test_data_dir);
    }

    #[actix_rt::test]
    async fn test_starting_data_logger_failure() {
        let test_data_dir = Path::new("/frank/nord");
        let data_logger = DataLogger::new(test_data_dir);
        assert!(data_logger.is_err());
    }
}
