use std::fmt::Debug;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use actix::prelude::{Context, Handler, Message};
use actix::Actor;
use faccess::{AccessMode, PathExt};
use uuid::Uuid;

use crate::errors::DataLogError;
use crate::messages::Publication;

/// Represents a message sent to the DataLogger via the DataLogRequest trait
#[derive(Debug, Message)]
#[rtype("Result<bool, DataLogError>")]
pub struct DataLogRequest {
    pub data_log_id: Uuid,
    pub data_log_entry: DataLogEntry,
}

/// The Actor responsible for executing DataLog requests sent by
/// PubSubServer actors.
/// ## Example
/// ```
/// # use std::fs::{ create_dir, remove_dir_all };
/// use std::path::Path;
/// use infotainer::data_log::DataLogger;
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
            Ok(DataLogger {
                data_dir: PathBuf::from(data_dir_path),
            })
        } else {
            Err(DataLogError::FileSystem(format!(
                "Could not access data directory with required permissions"
            )))
        }
    }

    fn write_data(&self, path: &PathBuf, log_entry: &Vec<u8>) -> Result<usize, DataLogError> {
        let rio = rio::new()?;
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(rio.write_at(&file, log_entry, 0).wait()?)
    }
}

impl Actor for DataLogger {
    type Context = Context<DataLogger>;
}

impl Handler<DataLogRequest> for DataLogger {
    type Result = Result<bool, DataLogError>;

    fn handle(
        &mut self,
        request: DataLogRequest,
        _: &mut Context<Self>,
    ) -> Result<bool, DataLogError> {
        let mut log_path = self.data_dir.join(&request.data_log_id.to_string());
        let log_data = match &request.data_log_entry {
            DataLogEntry::Item(a) => {
                log_path.set_file_name("subscribers");
                serde_cbor::to_vec(&a)?
            }
            DataLogEntry::CollectionItem(a) => {
                log_path.push("publications");
                log_path.set_file_name(format!("{}", a.id));
                serde_cbor::to_vec(&a)?
            }
        };
        let res = self.write_data(&log_path, &log_data)?;
        Ok(&res == &log_data.len())
    }
}

/// Holds data intended for writing. The variants indicate whether data should be held by a single file or by a file within a collection
#[derive(Debug, PartialEq)]
pub enum DataLogEntry {
    CollectionItem(Publication),
    Item(Vec<Uuid>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn create_test_directory() -> PathBuf {
        let mut p = temp_dir();
        p.push(format!("{}", Uuid::new_v4().to_hyphenated()));
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
    async fn test_data_log_request_submission() {
        let test_data_dir = create_test_directory();
        let dummy_data = (0..9).map(|_| Uuid::new_v4()).collect();
        let test_request = DataLogRequest {
            data_log_id: Uuid::new_v4(),
            data_log_entry: DataLogEntry::Item(dummy_data),
        };

        let data_logger = DataLogger::new(&test_data_dir).unwrap();
        let data_logger_actor = data_logger.start();

        let result = data_logger_actor.send(test_request).await;
        assert!(result.is_ok());
        remove_test_directory(&test_data_dir);
    }
}
