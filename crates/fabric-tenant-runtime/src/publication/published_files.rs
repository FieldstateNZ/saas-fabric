//! The published documents as mounted files, read as one generation.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::digest::sha256_hex;
use super::file_reads::{parse, read, read_optional};
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
    catalog: Option<PathBuf>,
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
            catalog: None,
        }
    }

    /// Also reads the catalogue at `catalog_path`, at start only:
    /// [`PublishedStateSource::load_at_start`]. A running replica keeps the
    /// catalogue it started with.
    #[must_use]
    pub fn with_catalogue(self, catalog_path: impl AsRef<Path>) -> Self {
        Self {
            catalog: Some(catalog_path.as_ref().to_path_buf()),
            ..self
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
        Ok(self.read(None).await?.0)
    }

    async fn load_at_start(&self) -> Result<(PublishedState, Option<Vec<u8>>), SourceError> {
        self.read(self.catalog.as_deref()).await
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

impl PublishedFiles {
    /// One read of the record and the documents it names, `catalog` among
    /// them when given. Without a record, the documents and the catalogue
    /// are read as they stand.
    async fn read(&self, catalog: Option<&Path>) -> Result<(PublishedState, Option<Vec<u8>>), SourceError> {
        let origin = self.describe();
        let Some(record) = read_optional(&self.record, &origin).await? else {
            let catalog = match catalog {
                Some(path) => Some(read(path, &origin).await?),
                None => None,
            };
            return Ok((self.unrecorded().await, catalog));
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
        let catalog = match catalog {
            Some(path) => Some(read(path, &origin).await?),
            None => None,
        };
        if let Some(bytes) = &catalog {
            if sha256_hex(bytes) != record.documents.catalog.sha256 {
                return Err(SourceError::IncompleteGeneration {
                    origin,
                    generation: record.generation,
                    document: "catalog",
                });
            }
        }
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

        let state = PublishedState::Generation {
            generation: record.generation,
            data_sources: parse(&data_sources, &origin)?,
            tenants: parse(&tenants, &origin)?,
        };
        Ok((state, catalog))
    }
}
