//! The published documents as mounted files, read as one generation.

use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use super::digest::sha256_hex;
use super::record::{ReaderRecord, PUBLICATION_RECORD_FILE, READER_CONTRACT};
use super::{PublishedState, PublishedStateSource};
use crate::{JsonFileSource, ResourceSource as _, SourceError};

/// The tenants and data-sources files, and the publication record beside the
/// tenants file.
///
/// The record is found beside `tenants.json` rather than configured: the
/// publisher writes it there, and in Kubernetes it is a key of the tenants
/// `ConfigMap`, so any mount that shows the tenants payload shows the record
/// with it, swapped in the same kubelet update.
pub struct PublishedFiles {
    tenants: PathBuf,
    data_sources: PathBuf,
    record: PathBuf,
}

impl PublishedFiles {
    /// Reads the two documents at the runtime's configured paths.
    #[must_use]
    pub fn new(tenants_path: impl AsRef<Path>, data_sources_path: impl AsRef<Path>) -> Self {
        let tenants = tenants_path.as_ref().to_path_buf();
        Self {
            record: tenants.with_file_name(PUBLICATION_RECORD_FILE),
            tenants,
            data_sources: data_sources_path.as_ref().to_path_buf(),
        }
    }

    async fn unrecorded(&self) -> PublishedState {
        PublishedState::Unrecorded {
            data_sources: JsonFileSource::new(&self.data_sources).load().await,
            tenants: JsonFileSource::new(&self.tenants).load().await,
        }
    }
}

#[async_trait]
impl PublishedStateSource for PublishedFiles {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let origin = self.describe();
        let Some(record) = read_optional(&self.record, &origin).await? else {
            return Ok(self.unrecorded().await);
        };
        let record: ReaderRecord = parse(&record, &origin)?;
        if record.minimum_reader_contract > READER_CONTRACT {
            return Err(SourceError::UnsupportedContract {
                origin,
                required: record.minimum_reader_contract,
                supported: READER_CONTRACT,
            });
        }

        let data_sources = read(&self.data_sources, &origin).await?;
        let tenants = read(&self.tenants, &origin).await?;
        for (document, bytes, named) in [
            (
                "data-sources",
                &data_sources,
                &record.documents.data_sources.sha256,
            ),
            ("tenants", &tenants, &record.documents.tenants.sha256),
        ] {
            if sha256_hex(bytes) != *named {
                return Err(SourceError::IncompleteGeneration {
                    origin,
                    generation: record.generation,
                    document,
                });
            }
        }

        Ok(PublishedState::Generation {
            generation: record.generation,
            data_sources: parse(&data_sources, &origin)?,
            tenants: parse(&tenants, &origin)?,
        })
    }

    fn describe(&self) -> String {
        format!(
            "published files: tenants={}, data_sources={}, record={}",
            self.tenants.display(),
            self.data_sources.display(),
            self.record.display()
        )
    }
}

async fn read_optional(path: &Path, origin: &str) -> Result<Option<Vec<u8>>, SourceError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(unreadable(origin, error)),
    }
}

async fn read(path: &Path, origin: &str) -> Result<Vec<u8>, SourceError> {
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

fn parse<T: DeserializeOwned>(bytes: &[u8], origin: &str) -> Result<T, SourceError> {
    serde_json::from_slice(bytes).map_err(|error| SourceError::Malformed {
        origin: origin.to_owned(),
        detail: error.to_string(),
    })
}
