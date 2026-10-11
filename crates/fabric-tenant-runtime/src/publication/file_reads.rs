//! Reading one published file, as the published-state readers word its
//! failures.

use std::io;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::SourceError;

pub(super) async fn read_optional(path: &Path, origin: &str) -> Result<Option<Vec<u8>>, SourceError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(unreadable(origin, error)),
    }
}

pub(super) async fn read(path: &Path, origin: &str) -> Result<Vec<u8>, SourceError> {
    tokio::fs::read(path)
        .await
        .map_err(|error| unreadable(origin, error))
}

fn unreadable(origin: &str, cause: io::Error) -> SourceError {
    SourceError::Unreadable {
        origin: origin.to_owned(),
        cause: Box::new(cause),
    }
}

pub(super) fn parse<T: DeserializeOwned>(bytes: &[u8], origin: &str) -> Result<T, SourceError> {
    serde_json::from_slice(bytes).map_err(|error| SourceError::Malformed {
        origin: origin.to_owned(),
        detail: error.to_string(),
    })
}
