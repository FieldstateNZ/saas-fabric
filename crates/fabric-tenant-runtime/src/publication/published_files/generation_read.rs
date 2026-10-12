//! One read of the published files, the catalogue among them or not.

use std::path::Path;

use super::PublishedFiles;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::file_reads::{read, read_optional};
use crate::publication::PublishedStateSource as _;
use crate::SourceError;

/// How a read treats the catalogue.
pub(super) enum Catalogue<'a> {
    /// Not read.
    Skip,
    /// Read, and the generation is refused unless it matches the record.
    Require(&'a Path),
    /// Read and kept only if it matches, to persist the generation whole.
    IfMatching(&'a Path),
}

impl PublishedFiles {
    /// One read of the record and the documents it names. `Ok(None)` when
    /// there is no record.
    pub(super) async fn read(
        &self,
        catalogue: Catalogue<'_>,
    ) -> Result<Option<CompleteGeneration>, SourceError> {
        let origin = self.describe();
        let Some(record) = read_optional(&self.record, &origin).await? else {
            return Ok(None);
        };
        let data_sources = read(&self.data_sources, &origin).await?;
        let tenants = read(&self.tenants, &origin).await?;
        let generation = match catalogue {
            Catalogue::Skip => CompleteGeneration::verify(record, data_sources, tenants, None, &origin)?,
            Catalogue::Require(path) => {
                let catalog = Some(read(path, &origin).await?);
                CompleteGeneration::verify(record, data_sources, tenants, catalog, &origin)?
            }
            Catalogue::IfMatching(path) => {
                CompleteGeneration::verify(record, data_sources, tenants, None, &origin)?
                    .attach_catalogue_if_matching(tokio::fs::read(path).await.ok())
            }
        };
        Ok(Some(generation))
    }
}
